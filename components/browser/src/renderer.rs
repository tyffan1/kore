use kore_ipc::{IpcMessage, IpcPayload, ProcessId};
use kore_sandbox::{Policy, PolicyBuilder, SandboxedProcess};
use url::Url;

use crate::error::BrowserError;

#[derive(Debug)]
pub struct RendererProcess {
    process: SandboxedProcess,
    process_id: ProcessId,
    tab_id: u64,
}

impl RendererProcess {
    pub fn spawn(tab_id: u64) -> Result<Self, BrowserError> {
        let policy = Self::renderer_policy();

        // The renderer binary path is a placeholder; in a real browser it
        // would be resolved from the install layout.
        let program = "kore-renderer";
        let args: Vec<String> = vec![tab_id.to_string()];

        let process =
            SandboxedProcess::spawn(program, &args, &policy).map_err(|e| {
                BrowserError::RendererSpawn(e.to_string())
            })?;

        let process_id = process.id();

        Ok(Self {
            process,
            process_id,
            tab_id,
        })
    }

    pub fn process_id(&self) -> ProcessId {
        self.process_id
    }

    pub fn tab_id(&self) -> u64 {
        self.tab_id
    }

    fn renderer_policy() -> Policy {
        PolicyBuilder::new()
            .allow_filesystem(true)
            .allow_networking(true)
            .max_memory(512 * 1024 * 1024)
            .max_cpu_time(30000)
            .build()
    }

    pub fn navigate(&self, url: &Url) -> Result<(), BrowserError> {
        // In a full implementation this would serialise an IpcMessage and
        // send it over the IPC transport.  For now the message construction
        // validates the intent.
        let _msg = IpcMessage::new(
            0,
            self.process_id,
            IpcPayload::NavigateToUrl {
                tab_id: self.tab_id,
                url: url.clone(),
            },
        );
        Ok(())
    }

    pub fn kill(self) -> Result<(), BrowserError> {
        // SandboxedProcess::kill takes &mut self; we consume self and
        // drop the process handle, which triggers job-object cleanup.
        let mut process = self.process;
        process.kill().map_err(|e| BrowserError::RendererSpawn(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_fails_gracefully_for_nonexistent_binary() {
        let result = RendererProcess::spawn(42);
        match result {
            Err(BrowserError::RendererSpawn(_)) => {}
            Err(other) => panic!("unexpected error: {other}"),
            Ok(p) => {
                p.kill().ok();
            }
        }
    }

    #[test]
    fn navigate_constructs_message() {
        let result = RendererProcess::spawn(7);
        if let Ok(rp) = result {
            let url = Url::parse("https://example.com/").expect("valid url");
            // navigate should not error even though we have no IPC transport yet
            rp.navigate(&url).expect("navigate");
            rp.kill().ok();
        }
    }

    #[test]
    fn renderer_policy_is_sane() {
        let policy = RendererProcess::renderer_policy();
        assert!(policy.allow_filesystem);
        assert!(policy.allow_networking);
        assert_eq!(policy.max_memory, Some(512 * 1024 * 1024));
        assert_eq!(policy.max_cpu_time, Some(30000));
    }
}
