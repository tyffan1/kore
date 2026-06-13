use thiserror::Error;

#[derive(Debug, Error)]
pub enum WindowError {
    #[error("failed to create winit event loop: {0}")]
    EventLoop(String),

    #[error("failed to create window: {0}")]
    Create(String),

    #[error("failed to create wgpu surface from window: {0}")]
    SurfaceCreate(String),
}
