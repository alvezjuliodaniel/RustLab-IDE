use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildProfile {
    #[default]
    Debug,
    Release,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
    Cobalt,
    SolarizedLight,
    SolarizedDark,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub theme: Theme,
    pub project: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    pub active: Option<usize>,
    #[serde(default)]
    pub project_profiles: BTreeMap<PathBuf, BuildProfile>,
}

fn session_path() -> io::Result<PathBuf> {
    let state_dir = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::home_dir().map(|home| home.join(".local/state")))
        .ok_or_else(|| io::Error::other("No se encontró el directorio de estado"))?;
    Ok(state_dir.join("rustlab/session-v1.json"))
}

impl Session {
    pub fn load() -> io::Result<Self> {
        match std::fs::read(session_path()?) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_at(&session_path()?)
    }

    fn save_at(&self, path: &Path) -> io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Ruta de sesión inválida"))?;
        std::fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer(&mut temp, self)?;
        temp.flush()?;
        temp.as_file().sync_all()?;
        temp.persist(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_session() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("session.json");
        let session = Session {
            project: Some(dir.path().join("proyecto 你好")),
            files: vec![dir.path().join("src/main.rs")],
            active: Some(0),
            theme: Theme::Cobalt,
            project_profiles: BTreeMap::from([(dir.path().to_owned(), BuildProfile::Release)]),
            recent_projects: vec![dir.path().to_owned()],
        };
        session.save_at(&path)?;
        let saved: Session = serde_json::from_slice(&std::fs::read(path)?)?;
        assert_eq!(saved.project, session.project);
        assert_eq!(saved.files, session.files);
        assert_eq!(saved.active, session.active);
        assert_eq!(saved.theme, session.theme);
        assert_eq!(saved.project_profiles, session.project_profiles);
        assert_eq!(saved.recent_projects, session.recent_projects);
        Ok(())
    }
}
