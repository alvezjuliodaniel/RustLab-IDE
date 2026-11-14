use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread;
use std::time::Duration;

pub struct Diagnostic {
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

pub enum Event {
    Output(String),
    Diagnostic(Diagnostic),
    Executable(PathBuf),
    Finished(io::Result<ExitStatus>, bool),
}

pub struct Job {
    cancel: Arc<AtomicBool>,
    pub events: Receiver<Event>,
}

impl Job {
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start(root: PathBuf, operation: &str, release: bool) -> Job {
    let mut command = Command::new("cargo");
    command.current_dir(&root).arg(operation);
    let structured = matches!(operation, "check" | "build" | "run" | "test" | "clippy");
    if structured {
        command.arg("--message-format=json");
    }
    if release && (structured || operation == "doc") {
        command.arg("--release");
    }
    let (sender, events) = sync_channel(256);
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    thread::spawn(move || {
        let mut cancelled = false;
        let result = execute(command, &root, structured, &sender, &flag, &mut cancelled);
        let _ = sender.send(Event::Finished(result, cancelled));
    });
    Job { cancel, events }
}

fn execute(
    mut command: Command,
    root: &Path,
    structured: bool,
    sender: &SyncSender<Event>,
    cancel: &AtomicBool,
    cancelled: &mut bool,
) -> io::Result<ExitStatus> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");
    let stdout_sender = sender.clone();
    let root = root.to_owned();
    let stdout = thread::spawn(move || read_output(stdout, &root, structured, &stdout_sender));
    let stderr_sender = sender.clone();
    let stderr = thread::spawn(move || read_output(stderr, Path::new(""), false, &stderr_sender));
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            *cancelled = true;
            #[cfg(unix)]
            {
                // The child owns the process group and has not been reaped yet.
                unsafe { libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL) };
            }
            let _ = child.kill();
        }
        if stdout.is_finished()
            && stderr.is_finished()
            && let Some(status) = child.try_wait()?
        {
            break status;
        }
        thread::sleep(Duration::from_millis(30));
    };
    // Let readers flush Cargo's final messages before delivering the result.
    for reader in [stdout, stderr] {
        reader
            .join()
            .map_err(|_| io::Error::other("Falló la lectura de Cargo"))??;
    }
    Ok(status)
}

fn read_output(
    reader: impl Read,
    root: &Path,
    structured: bool,
    sender: &SyncSender<Event>,
) -> io::Result<()> {
    let mut reader = BufReader::new(reader);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        if reader
            .by_ref()
            .take(1024 * 1024)
            .read_until(b'\n', &mut bytes)?
            == 0
        {
            return Ok(());
        }
        let line = String::from_utf8_lossy(&bytes).replace('\0', "�");
        if structured && let Some(events) = cargo_message(root, &line) {
            for event in events {
                if sender.send(event).is_err() {
                    return Ok(());
                }
            }
        } else if sender.send(Event::Output(line)).is_err() {
            return Ok(());
        }
    }
}

fn cargo_message(root: &Path, line: &str) -> Option<Vec<Event>> {
    let json: serde_json::Value = serde_json::from_str(line).ok()?;
    let mut events = Vec::new();
    match json["reason"].as_str()? {
        "compiler-artifact" => {
            if let Some(path) = json["executable"].as_str() {
                events.push(Event::Executable(root.join(path)));
            }
        }
        "compiler-message" => {
            let message = &json["message"];
            if let Some(rendered) = message["rendered"].as_str() {
                events.push(Event::Output(rendered.to_owned()));
            }
            if let Some(spans) = message["spans"].as_array() {
                for span in spans.iter().filter(|span| span["is_primary"] == true) {
                    if let (Some(path), Some(line), Some(column), Some(message)) = (
                        span["file_name"].as_str(),
                        span["line_start"].as_u64(),
                        span["column_start"].as_u64(),
                        message["message"].as_str(),
                    ) && let (Ok(line), Ok(column)) =
                        (u32::try_from(line), u32::try_from(column))
                    {
                        events.push(Event::Diagnostic(Diagnostic {
                            path: root.join(path),
                            line,
                            column,
                            message: message.to_owned(),
                        }));
                    }
                }
            }
        }
        "build-finished" | "build-script-executed" => {}
        _ => return None,
    }
    Some(events)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn streams_output_and_cancels_a_running_process_group() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("descendant");
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "(sleep 1; touch descendant) & printf 'listo\\n'; wait",
            ])
            .current_dir(dir.path());
        let (sender, receiver) = sync_channel(8);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let runner = thread::spawn(move || {
            let mut cancelled = false;
            let status = execute(
                command,
                Path::new(""),
                false,
                &sender,
                &flag,
                &mut cancelled,
            )
            .unwrap();
            (status, cancelled)
        });
        let output = receiver.recv_timeout(Duration::from_secs(3));
        cancel.store(true, Ordering::Relaxed);
        assert!(matches!(output, Ok(Event::Output(line)) if line == "listo\n"));
        let (status, cancelled) = runner.join().unwrap();
        assert!(!status.success() && cancelled);
        thread::sleep(Duration::from_millis(1100));
        assert!(!marker.exists());
    }
}
