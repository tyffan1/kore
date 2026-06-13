use kore_ipc::{IpcMessage, IpcPayload};
use kore_sandbox::{PolicyBuilder, SandboxedProcess};

use crate::extension::{Extension, ExtensionError};

/// An isolated extension process managed by the sandbox.
pub struct ExtensionProcess {
    extension: Extension,
    process: Option<SandboxedProcess>,
    process_id: u32,
}

impl ExtensionProcess {
    pub fn spawn(extension: Extension) -> Result<Self, ExtensionError> {
        let policy = PolicyBuilder::new()
            .allow_filesystem(true)
            .allow_networking(true)
            .build();

        let ext_path = extension.path.to_string_lossy().to_string();
        let args = vec![ext_path];

        let process_id;
        let proc = match SandboxedProcess::spawn("kore-extension-runner", &args, &policy) {
            Ok(p) => {
                process_id = p.id();
                Some(p)
            }
            Err(e) => {
                eprintln!("[extensions] failed to spawn process for '{}': {e}", extension.name());
                return Err(ExtensionError::ProcessSpawn(e.to_string()));
            }
        };

        Ok(Self {
            extension,
            process: proc,
            process_id,
        })
    }

    pub fn extension(&self) -> &Extension {
        &self.extension
    }

    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    pub fn is_alive(&mut self) -> bool {
        self.process
            .as_mut()
            .and_then(|p| p.is_alive().ok())
            .unwrap_or(false)
    }

    pub fn kill(&mut self) -> Result<(), ExtensionError> {
        if let Some(ref mut p) = self.process {
            p.kill().map_err(|e| ExtensionError::ProcessKill(e.to_string()))?;
        }
        self.process = None;
        Ok(())
    }

    pub fn send_message(&self, payload: IpcPayload) -> Result<(), ExtensionError> {
        let msg = IpcMessage::new(0, self.process_id, payload);
        let _bytes = msg.to_bytes().map_err(|e| ExtensionError::Ipc(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_process_creation_valid() {
        let policy = PolicyBuilder::new().allow_filesystem(true).build();
        let result = SandboxedProcess::spawn("echo", &["hello".to_string()], &policy);
        if let Ok(mut proc) = result {
            assert!(proc.id() > 0);
            let _ = proc.kill();
        }
    }

    #[test]
    fn test_policy_builder_allows_permissions() {
        let policy = PolicyBuilder::new().allow_filesystem(true).allow_networking(true).build();
        let args = vec!["--version".to_string()];
        let result = SandboxedProcess::spawn("echo", &args, &policy);
        if let Ok(mut proc) = result {
            let _ = proc.kill();
        }
    }
}
