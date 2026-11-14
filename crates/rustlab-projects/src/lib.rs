use serde::Deserialize;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Clone, Copy)]
pub enum Template {
    Console,
    Library,
    Workspace,
    Gtk,
    CortexM,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub workspace_root: PathBuf,
    pub packages: Vec<Package>,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub manifest_path: PathBuf,
    pub targets: Vec<Target>,
}

#[derive(Debug, Deserialize)]
pub struct Target {
    pub name: String,
    pub kind: Vec<String>,
}

impl Project {
    pub fn open(path: &Path) -> io::Result<Self> {
        let manifest = if path.is_dir() {
            path.join("Cargo.toml")
        } else {
            path.to_owned()
        };
        let manifest = manifest.canonicalize()?;
        let output = Command::new("cargo")
            .args([
                "metadata",
                "--format-version",
                "1",
                "--no-deps",
                "--offline",
                "--manifest-path",
            ])
            .arg(&manifest)
            .current_dir(
                manifest
                    .parent()
                    .ok_or_else(|| io::Error::other("Manifiesto sin carpeta"))?,
            )
            .output()?;
        serde_json::from_slice(&successful(output)?).map_err(io::Error::other)
    }

    pub fn create(parent: &Path, name: &str, template: Template) -> io::Result<Self> {
        if !name
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic())
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            return Err(io::Error::other(
                "El nombre debe empezar con una letra ASCII y puede contener números, guiones y guiones bajos",
            ));
        }
        let destination = parent.canonicalize()?.join(name);
        std::fs::create_dir(&destination)?;
        write_template(&destination, name, template)?;
        Self::open(&destination)
    }
}

fn write_template(root: &Path, name: &str, template: Template) -> io::Result<()> {
    use std::fs::{create_dir, write};

    let workspace = "[workspace]\nresolver = \"3\"\n";
    if matches!(template, Template::Workspace) {
        return write(root.join("Cargo.toml"), workspace);
    }
    let mut manifest = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n{workspace}"
    );
    let (file, contents) = match template {
        Template::Console => (
            "main.rs",
            "fn main() {\n    println!(\"¡Hola desde Rust!\");\n}\n",
        ),
        Template::Library => (
            "lib.rs",
            "pub fn sumar(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
        ),
        Template::Gtk => {
            manifest.push_str("\n[dependencies]\ngtk = { package = \"gtk4\", version = \"=0.10.3\" }\nadw = { package = \"libadwaita\", version = \"=0.8.0\" }\n");
            ("main.rs", include_str!("../templates/gtk.rs"))
        }
        Template::CortexM => {
            manifest.push_str("\n[dependencies]\ncortex-m-rt = \"=0.7.7\"\n");
            create_dir(root.join(".cargo"))?;
            write(
                root.join(".cargo/config.toml"),
                "[build]\ntarget = \"thumbv7em-none-eabihf\"\n\n[target.thumbv7em-none-eabihf]\nrustflags = [\"-C\", \"link-arg=-Tlink.x\"]\n",
            )?;
            write(
                root.join("memory.x"),
                "/* STM32F303VCT6 */\nMEMORY {\n  FLASH : ORIGIN = 0x08000000, LENGTH = 256K\n  RAM : ORIGIN = 0x20000000, LENGTH = 40K\n}\n",
            )?;
            write(
                root.join("build.rs"),
                "fn main() {\n    println!(\"cargo:rustc-link-search={}\", env!(\"CARGO_MANIFEST_DIR\"));\n    println!(\"cargo:rerun-if-changed=memory.x\");\n}\n",
            )?;
            ("main.rs", include_str!("../templates/cortex-m.rs"))
        }
        Template::Workspace => unreachable!(),
    };
    create_dir(root.join("src"))?;
    write(root.join("src").join(file), contents)?;
    write(root.join("Cargo.toml"), manifest)
}

fn successful(output: Output) -> io::Result<Vec<u8>> {
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

/// Create a file alongside existing project files without overwriting them.
pub fn create_file(directory: &Path, name: &str) -> io::Result<PathBuf> {
    use std::path::Component;
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || Path::new(name).file_name() != Some(std::ffi::OsStr::new(name))
    {
        return Err(io::Error::other(
            "Ingresá un nombre de archivo, sin carpetas",
        ));
    }
    let file = directory.join(name);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file)?;
    Ok(file)
}

/// Prepare an external module for a conventional crate root or a mod.rs file.
pub fn create_module(parent: &Path, source: &str, name: &str) -> io::Result<PathBuf> {
    let mut bytes = name.bytes();
    if name == "gen"
        || !bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        || !bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        || syn::parse_str::<syn::Ident>(name).is_err()
    {
        return Err(io::Error::other(
            "Ingresá un identificador Rust válido en minúsculas",
        ));
    }
    if !matches!(
        parent.file_name().and_then(|name| name.to_str()),
        Some("lib.rs" | "main.rs" | "mod.rs")
    ) {
        return Err(io::Error::other(
            "Abrí la raíz convencional del crate o un mod.rs",
        ));
    }
    let syntax = syn::parse_file(source).map_err(io::Error::other)?;
    if syntax.items.iter().any(|item| {
        matches!(item, syn::Item::Mod(module)
            if module.ident.to_string().trim_start_matches("r#") == name)
    }) {
        return Err(io::Error::other(
            "Ya existe una declaración para ese módulo",
        ));
    }
    let directory = parent
        .parent()
        .ok_or_else(|| io::Error::other("Archivo sin carpeta"))?;
    match std::fs::symlink_metadata(directory.join(name).join("mod.rs")) {
        Ok(_) => return Err(io::Error::other("Ya existe un mod.rs para ese módulo")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    create_file(directory, &format!("{name}.rs"))
}

pub fn duplicate_project(source: &Path, destination: &Path) -> io::Result<Project> {
    let source = source.canonicalize()?;
    let parent = destination
        .parent()
        .ok_or_else(|| io::Error::other("Destino sin carpeta superior"))?;
    let name = destination
        .file_name()
        .ok_or_else(|| io::Error::other("Destino sin nombre"))?;
    let parent = parent.canonicalize()?;
    if parent.starts_with(&source) {
        return Err(io::Error::other(
            "Elegí un destino fuera del proyecto original",
        ));
    }
    let destination = parent.join(name);
    std::fs::create_dir(&destination)?;
    if let Err(error) = copy_tree(&source, &destination, true) {
        return Err(io::Error::other(format!(
            "Copia incompleta en {}: {error}",
            destination.display()
        )));
    }
    Project::open(&destination)
}

fn copy_tree(source: &Path, destination: &Path, root: bool) -> io::Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" || (root && name == "target") {
            continue;
        }
        let kind = entry.file_type()?;
        let target = destination.join(name);
        if kind.is_dir() {
            std::fs::create_dir(&target)?;
            copy_tree(&entry.path(), &target, false)?;
        } else if kind.is_file() {
            let mut input = std::fs::File::open(entry.path())?;
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            io::copy(&mut input, &mut output)?;
            output.set_permissions(input.metadata()?.permissions())?;
        } else {
            return Err(io::Error::other(format!(
                "Tipo de archivo sin soporte: {}",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_module_for_a_real_crate() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let project = Project::create(root.path(), "modulos", Template::Library)?;
        let parent = project.workspace_root.join("src/lib.rs");
        let source = std::fs::read_to_string(&parent)?;
        assert!(create_module(&parent, &source, "fn").is_err());
        assert!(create_module(&parent, "mod r#datos {}", "datos").is_err());
        let child = create_module(&parent, &source, "datos")?;
        assert_eq!(child, project.workspace_root.join("src/datos.rs"));
        assert!(create_module(&parent, &source, "datos").is_err());
        std::fs::write(parent, format!("{source}\nmod datos;\n"))?;
        successful(
            Command::new("cargo")
                .args(["check", "--offline"])
                .current_dir(&project.workspace_root)
                .output()?,
        )?;
        Ok(())
    }

    #[test]
    fn duplicates_project_without_overwriting_or_copying_build_artifacts() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let project = Project::create(root.path(), "original", Template::Console)?;
        let original = &project.workspace_root;
        std::fs::create_dir(original.join("target"))?;
        let copy = root.path().join("segunda copia");
        let duplicate = duplicate_project(original, &copy)?;
        assert_eq!(duplicate.workspace_root, copy);
        assert_eq!(
            std::fs::read(copy.join("src/main.rs"))?,
            std::fs::read(original.join("src/main.rs"))?
        );
        assert!(!copy.join("target").exists());
        assert!(duplicate_project(original, &copy).is_err());
        assert!(duplicate_project(original, &original.join("interna")).is_err());
        Ok(())
    }

    #[test]
    fn imports_virtual_workspace_with_multiple_targets() -> io::Result<()> {
        let folder = tempfile::tempdir()?;
        let root = folder.path().join("workspace con espacios");
        std::fs::create_dir(&root)?;
        let member = root.join("app");
        std::fs::create_dir_all(member.join("src/bin"))?;
        std::fs::create_dir(member.join("examples"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver = \"3\"\nmembers = [\"app\"]\n",
        )?;
        std::fs::write(
            member.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        for file in [
            "src/main.rs",
            "src/bin/secundario.rs",
            "examples/muestra.rs",
        ] {
            std::fs::write(member.join(file), "fn main() {}")?;
        }
        let project = Project::open(&member.join("Cargo.toml"))?;
        assert_eq!(project.workspace_root, root.canonicalize()?);
        let package = &project.packages[0];
        assert_eq!(package.name, "app");
        for (name, kind) in [
            ("app", "bin"),
            ("secundario", "bin"),
            ("muestra", "example"),
        ] {
            assert!(
                package
                    .targets
                    .iter()
                    .any(|target| target.name == name && target.kind == [kind])
            );
        }
        Ok(())
    }

    #[test]
    fn new_file_protects_existing_files_and_parent() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let path = create_file(root.path(), "modulo.rs")?;
        std::fs::write(&path, "pub fn ejemplo() {}")?;
        assert!(create_file(root.path(), "modulo.rs").is_err());
        assert!(create_file(root.path(), "../otro.rs").is_err());
        assert_eq!(std::fs::read_to_string(path)?, "pub fn ejemplo() {}");
        Ok(())
    }

    #[test]
    #[ignore = "requiere GTK4 y el target thumbv7em-none-eabihf"]
    fn builds_specialized_templates() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        for (name, template) in [
            ("prueba_gui", Template::Gtk),
            ("prueba_embebida", Template::CortexM),
        ] {
            let project = Project::create(root.path(), name, template)?;
            let output = Command::new("cargo")
                .args(["build", "--message-format=json"])
                .current_dir(project.workspace_root)
                .env("CARGO_TARGET_DIR", root.path().join("artifacts"))
                .env("CARGO_PROFILE_DEV_DEBUG", "0")
                .output()?;
            let messages = successful(output)?;
            assert!(
                String::from_utf8_lossy(&messages).lines().any(|line| {
                    serde_json::from_str::<serde_json::Value>(line).is_ok_and(|message| {
                        message["reason"] == "compiler-artifact"
                            && message["target"]["name"] == name
                            && message["executable"]
                                .as_str()
                                .is_some_and(|path| Path::new(path).is_file())
                    })
                }),
                "Cargo no emitió un ejecutable para {name}"
            );
        }
        Ok(())
    }

    #[test]
    fn creates_real_projects_and_preserves_existing_files() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let app = Project::create(root.path(), "prueba_app", Template::Console)?;
        let lib = Project::create(root.path(), "prueba_lib", Template::Library)?;
        let workspace = Project::create(root.path(), "prueba_workspace", Template::Workspace)?;
        assert!(workspace.packages.is_empty());
        let gtk = Project::create(root.path(), "prueba_gui", Template::Gtk)?;
        let embedded = Project::create(root.path(), "prueba_embebida", Template::CortexM)?;
        assert_eq!(gtk.packages[0].targets[0].kind, ["bin"]);
        assert!(embedded.workspace_root.join("memory.x").is_file());
        assert_eq!(app.packages[0].targets[0].kind, ["bin"]);
        assert_eq!(lib.packages[0].targets[0].kind, ["lib"]);
        assert!(Project::create(root.path(), "prueba_app", Template::Library).is_err());
        assert!(app.workspace_root.join("src/main.rs").is_file());
        Ok(())
    }
}
