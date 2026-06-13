use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("failed to spawn sandboxed process: {0}")]
    Spawn(String),

    #[error("failed to kill process: {0}")]
    Kill(String),

    #[error("process check failed: {0}")]
    ProcessCheck(String),

    #[error("sandboxing not supported on this platform")]
    UnsupportedPlatform,
}
