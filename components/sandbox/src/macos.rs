use crate::error::SandboxError;
use crate::policy::Policy;

#[derive(Debug)]
pub(crate) struct ProcessHandle;

pub(crate) fn spawn(
    _program: &str,
    _args: &[String],
    _policy: &Policy,
) -> Result<(u32, ProcessHandle), SandboxError> {
    Err(SandboxError::UnsupportedPlatform)
}

pub(crate) fn kill(_handle: &mut ProcessHandle) -> Result<(), SandboxError> {
    Err(SandboxError::UnsupportedPlatform)
}

pub(crate) fn is_alive(_handle: &mut ProcessHandle) -> Result<bool, SandboxError> {
    Err(SandboxError::UnsupportedPlatform)
}
