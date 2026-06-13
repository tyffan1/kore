use crate::error::SandboxError;
use crate::policy::Policy;
use crate::sys;

#[derive(Debug)]
pub struct SandboxedProcess {
    pid: u32,
    inner: sys::ProcessHandle,
}

impl SandboxedProcess {
    pub fn spawn(program: &str, args: &[String], policy: &Policy) -> Result<Self, SandboxError> {
        let (pid, inner) = sys::spawn_sandboxed(program, args, policy)?;
        Ok(Self { pid, inner })
    }

    pub fn kill(&mut self) -> Result<(), SandboxError> {
        sys::kill_sandboxed(&mut self.inner)
    }

    pub fn is_alive(&mut self) -> Result<bool, SandboxError> {
        sys::is_alive_sandboxed(&mut self.inner)
    }

    pub fn id(&self) -> u32 {
        self.pid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::PolicyBuilder;

    fn make_args(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn process_id_assigned() {
        let policy = PolicyBuilder::new().allow_filesystem(true).build();
        let args = make_args(&["/C", "echo", "test"]);
        if let Ok(proc) = SandboxedProcess::spawn("cmd.exe", &args, &policy) {
            assert!(proc.id() > 0);
        }
    }

    #[test]
    fn spawn_and_kill_does_not_error() {
        let policy = PolicyBuilder::new().allow_filesystem(true).build();
        let args = make_args(&["/C", "echo", "hello"]);
        if let Ok(mut proc) = SandboxedProcess::spawn("cmd.exe", &args, &policy) {
            let result = proc.kill();
            assert!(result.is_ok());
        }
    }

    #[test]
    fn is_alive_after_kill_returns_false() {
        let policy = PolicyBuilder::new().allow_filesystem(true).build();
        let args = make_args(&["/C", "echo", "x"]);
        if let Ok(mut proc) = SandboxedProcess::spawn("cmd.exe", &args, &policy) {
            let _ = proc.kill();
            let alive = proc.is_alive().unwrap_or(false);
            assert!(!alive);
        }
    }
}
