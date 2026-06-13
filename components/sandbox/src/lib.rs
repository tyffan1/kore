//! OS-level process sandbox abstraction for Kore.

mod error;
mod policy;
mod process;
mod sys;

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "macos")]
mod macos;

pub use error::SandboxError;
pub use policy::{Policy, PolicyBuilder, SyscallAction, SyscallRule};
pub use process::SandboxedProcess;
