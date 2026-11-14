use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

pub struct Client {
    child: Child,
    input: ChildStdin,
    messages: Receiver<Value>,
    versions: HashMap<String, u64>,
    next_id: u64,
}

impl Client {
    pub fn start(root: &Path, uri: &str) -> io::Result<Self> {
        let mut child = Command::new("rust-analyzer")
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, messages) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            while let Ok(message) = read_message(&mut reader) {
                if tx.send(message).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            input,
            messages,
            versions: HashMap::new(),
            next_id: 0,
        };
        client.request("initialize", json!({
            "processId": std::process::id(),
            "rootUri": uri,
            "workspaceFolders": [{"uri":uri,"name":root.file_name().unwrap_or_default().to_string_lossy()}],
            "capabilities":{"textDocument":{"completion":{"completionItem":{"snippetSupport":true,"resolveSupport":{"properties":["additionalTextEdits"]}}}}},
            "initializationOptions":{"checkOnSave":false,"cargo":{"buildScripts":{"enable":false}},"procMacro":{"enable":false}}
        }))?;
        client.notify("initialized", json!({}))?;
        Ok(client)
    }

    pub fn complete(
        &mut self,
        uri: &str,
        text: &str,
        line: u32,
        character: u32,
    ) -> io::Result<Vec<Value>> {
        if let Some(version) = self.versions.get_mut(uri) {
            *version += 1;
            let version = *version;
            self.notify(
                "textDocument/didChange",
                json!({
                    "textDocument":{"uri":uri,"version":version},
                    "contentChanges":[{"text":text}]
                }),
            )?;
        } else {
            self.notify(
                "textDocument/didOpen",
                json!({
                    "textDocument":{"uri":uri,"languageId":"rust","version":1,"text":text}
                }),
            )?;
            self.versions.insert(uri.to_owned(), 1);
        }
        let result = self.request(
            "textDocument/completion",
            json!({
                "textDocument":{"uri":uri},"position":{"line":line,"character":character}
            }),
        )?;
        Ok(result
            .as_array()
            .or_else(|| result["items"].as_array())
            .cloned()
            .unwrap_or_default())
    }

    pub fn close(&mut self, uri: &str) {
        if self.versions.remove(uri).is_some() {
            let _ = self.notify("textDocument/didClose", json!({"textDocument":{"uri":uri}}));
        }
    }

    fn notify(&mut self, method: &str, params: Value) -> io::Result<()> {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}))
    }

    fn request(&mut self, method: &str, params: Value) -> io::Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        let deadline = Instant::now() + Duration::from_secs(8);
        while let Ok(message) = self
            .messages
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            if message.get("method").is_some() {
                if let Some(id) = message.get("id") {
                    self.send(json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not supported"}}))?;
                }
                continue;
            }
            if message["id"] == id {
                if let Some(error) = message.get("error") {
                    return Err(io::Error::other(error.to_string()));
                }
                return Ok(message["result"].clone());
            }
        }
        let _ = self.notify("$/cancelRequest", json!({"id":id}));
        Err(io::Error::other("rust-analyzer no respondió"))
    }

    fn send(&mut self, message: Value) -> io::Result<()> {
        let bytes = serde_json::to_vec(&message)?;
        write!(self.input, "Content-Length: {}\r\n\r\n", bytes.len())?;
        self.input.write_all(&bytes)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let length = length
        .filter(|n| *n <= 16 * 1024 * 1024)
        .ok_or_else(|| io::Error::other("Mensaje LSP inválido"))?;
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub enum Task {
    Complete {
        uri: String,
        text: String,
        line: u32,
        character: u32,
        reply: Sender<io::Result<Vec<Value>>>,
    },
    Resolve(Value, Sender<io::Result<Value>>),
    Close(String),
}

pub fn worker(root: std::path::PathBuf, uri: String) -> Sender<Task> {
    let (send, receive) = channel();
    std::thread::spawn(move || {
        let mut client = None;
        while let Ok(task) = receive.recv() {
            match task {
                Task::Complete {
                    uri: document,
                    text,
                    line,
                    character,
                    reply,
                } => {
                    let result = (|| {
                        if client.is_none() {
                            client = Some(Client::start(&root, &uri)?);
                        }
                        client
                            .as_mut()
                            .unwrap()
                            .complete(&document, &text, line, character)
                    })();
                    if result.is_err() {
                        client.take();
                    }
                    let _ = reply.send(result);
                }
                Task::Resolve(item, reply) => {
                    let result = client
                        .as_mut()
                        .ok_or_else(|| io::Error::other("Sesión LSP cerrada"))
                        .and_then(|client| client.request("completionItem/resolve", item));
                    let _ = reply.send(result);
                }
                Task::Close(uri) => {
                    if let Some(client) = client.as_mut() {
                        client.close(&uri);
                    }
                }
            }
        }
    });
    send
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requiere rust-analyzer y un proyecto Cargo real"]
    fn returns_semantic_completions_for_unsaved_rust() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='completado'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        let file = root.path().join("src/lib.rs");
        std::fs::write(&file, "").unwrap();
        let mut client =
            Client::start(root.path(), &format!("file://{}", root.path().display())).unwrap();
        let text = "struct Punto { coordenada: u32 }\nfn ejemplo(punto: Punto) { punto.coor }";
        let column = text.lines().nth(1).unwrap().find("coor").unwrap() as u32 + 4;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let result = client
                .complete(&format!("file://{}", file.display()), text, 1, column)
                .unwrap();
            if result.iter().any(|item| item["label"] == "coordenada") {
                break;
            }
            assert!(Instant::now() < deadline, "{result:?}");
            std::thread::sleep(Duration::from_millis(250));
        }
        let text = "mod interno { pub struct EjemploImportado; }\nfn uso() { let _ = EjemploIm; }";
        let column = text.lines().nth(1).unwrap().find("EjemploIm").unwrap() as u32 + 9;
        let result = client
            .complete(&format!("file://{}", file.display()), text, 1, column)
            .unwrap();
        let item = result
            .into_iter()
            .find(|item| {
                item["label"]
                    .as_str()
                    .is_some_and(|label| label.starts_with("EjemploImportado"))
            })
            .expect("sugerencia importable");
        let resolved = if item.get("additionalTextEdits").is_some() {
            item
        } else {
            client.request("completionItem/resolve", item).unwrap()
        };
        assert!(
            resolved["additionalTextEdits"]
                .to_string()
                .contains("use crate::interno::EjemploImportado;"),
            "{resolved}"
        );
    }
}
