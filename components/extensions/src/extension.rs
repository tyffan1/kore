use std::path::PathBuf;

use crate::manifest::{Manifest, ManifestError};

pub type ExtensionId = String;

#[derive(Debug, Clone)]
pub struct Extension {
    pub id: ExtensionId,
    pub manifest: Manifest,
    pub path: PathBuf,
    pub enabled: bool,
}

impl Extension {
    pub fn load(path: PathBuf) -> Result<Self, ExtensionError> {
        let manifest_path = path.join("manifest.json");
        let manifest_content =
            std::fs::read_to_string(&manifest_path).map_err(|e| ExtensionError::Io {
                path: manifest_path.clone(),
                source: e,
            })?;
        let manifest = Manifest::parse(&manifest_content)?;
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| ExtensionError::InvalidPath(path.clone()))?;
        Ok(Self {
            id,
            manifest,
            path,
            enabled: true,
        })
    }

    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    pub fn version(&self) -> &str {
        &self.manifest.version
    }
}

#[derive(Debug)]
pub enum ExtensionError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Manifest(ManifestError),
    InvalidPath(PathBuf),
    NotFound(String),
    ProcessSpawn(String),
    ProcessKill(String),
    Ipc(String),
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtensionError::Io { path, source } => {
                write!(f, "error reading {}: {source}", path.display())
            }
            ExtensionError::Manifest(e) => write!(f, "{e}"),
            ExtensionError::InvalidPath(p) => {
                write!(f, "invalid extension path: {}", p.display())
            }
            ExtensionError::NotFound(id) => write!(f, "extension not found: {id}"),
            ExtensionError::ProcessSpawn(msg) => write!(f, "failed to spawn extension process: {msg}"),
            ExtensionError::ProcessKill(msg) => write!(f, "failed to kill extension process: {msg}"),
            ExtensionError::Ipc(msg) => write!(f, "IPC error: {msg}"),
        }
    }
}

impl std::error::Error for ExtensionError {}

impl From<ManifestError> for ExtensionError {
    fn from(e: ManifestError) -> Self {
        ExtensionError::Manifest(e)
    }
}
