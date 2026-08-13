use thiserror::Error;

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("tab not found: {0}")]
    TabNotFound(u64),

    #[error("failed to spawn renderer process: {0}")]
    RendererSpawn(String),

    #[error("renderer communication error: {0}")]
    RendererIpc(String),

    #[error("failed to spawn network process: {0}")]
    NetworkSpawn(String),

    #[error("network process communication error: {0}")]
    NetworkIpc(String),

    #[error("failed to spawn GPU process: {0}")]
    GpuSpawn(String),

    #[error("GPU process communication error: {0}")]
    GpuIpc(String),

    #[error("session error: {0}")]
    Session(String),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("network error: {0}")]
    Net(#[from] kore_net::HttpError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
