use crate::error::SandboxError;
use crate::policy::Policy;

#[cfg(windows)]
mod platform_impl {
    pub(crate) use crate::windows::{is_alive, kill, spawn, ProcessHandle};
}

#[cfg(target_os = "linux")]
mod platform_impl {
    pub(crate) use crate::linux::{is_alive, kill, spawn, ProcessHandle};
}

#[cfg(target_os = "macos")]
mod platform_impl {
    pub(crate) use crate::macos::{is_alive, kill, spawn, ProcessHandle};
}

pub(crate) use platform_impl::{is_alive, kill, spawn, ProcessHandle};

pub(crate) fn spawn_sandboxed(
    program: &str,
    args: &[String],
    policy: &Policy,
) -> Result<(u32, ProcessHandle), SandboxError> {
    spawn(program, args, policy)
}

pub(crate) fn kill_sandboxed(handle: &mut ProcessHandle) -> Result<(), SandboxError> {
    kill(handle)
}

pub(crate) fn is_alive_sandboxed(handle: &mut ProcessHandle) -> Result<bool, SandboxError> {
    is_alive(handle)
}
