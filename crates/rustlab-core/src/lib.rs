use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub struct Document {
    path: PathBuf,
    original: Vec<u8>,
    bom: bool,
    crlf: bool,
}

impl Document {
    pub fn open(path: &Path) -> io::Result<(Self, String)> {
        let path = path.canonicalize()?;
        if !fs::metadata(&path)?.is_file() {
            return Err(io::Error::other(
                "La ruta no corresponde a un archivo regular",
            ));
        }
        let original = fs::read(&path)?;
        let bom = original.starts_with(b"\xef\xbb\xbf");
        let text =
            std::str::from_utf8(if bom { &original[3..] } else { &original }).map_err(|_| {
                io::Error::other("Codificación no compatible. Se necesita un archivo UTF-8")
            })?;
        if text.contains('\0') {
            return Err(io::Error::other("El archivo contiene datos binarios"));
        }
        let crlf = text.contains("\r\n") && !text.replace("\r\n", "").contains('\n');
        let text = if crlf {
            text.replace("\r\n", "\n")
        } else {
            text.to_owned()
        };
        Ok((
            Self {
                path,
                original,
                bom,
                crlf,
            },
            text,
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn save(&mut self, text: &str) -> io::Result<()> {
        let mut bytes = Vec::new();
        if self.bom {
            bytes.extend(b"\xef\xbb\xbf");
        }
        if self.crlf {
            bytes.extend(text.replace('\n', "\r\n").as_bytes());
        } else {
            bytes.extend(text.as_bytes());
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| io::Error::other("Ruta sin carpeta"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary
            .as_file()
            .set_permissions(fs::metadata(&self.path)?.permissions())?;
        temporary.as_file().sync_all()?;
        if fs::read(&self.path)? != self.original {
            return Err(io::Error::other(
                "El archivo cambió en disco. Conservá la edición y revisá el cambio externo",
            ));
        }
        temporary.persist(&self.path)?;
        self.original = bytes;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_bom_and_crlf_when_saving() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("main.rs");
        fs::write(&path, b"\xef\xbb\xbfuno\r\n")?;
        let (mut document, text) = Document::open(&path)?;
        assert_eq!(text, "uno\n");
        document.save("dos\ntres\n")?;
        assert_eq!(fs::read(path)?, b"\xef\xbb\xbfdos\r\ntres\r\n");
        Ok(())
    }

    #[test]
    fn protects_external_edits() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("main.rs");
        fs::write(&path, "original")?;
        let (mut document, _) = Document::open(&path)?;
        fs::write(&path, "externo")?;
        assert!(document.save("editor").is_err());
        assert_eq!(fs::read_to_string(path)?, "externo");
        Ok(())
    }
}
