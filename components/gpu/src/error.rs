use thiserror::Error;

/// Errors that can occur during GPU initialisation or rendering.
#[derive(Debug, Error)]
pub enum GpuError {
    /// No suitable GPU adapter was found.
    #[error("no suitable GPU adapter found")]
    NoAdapter,

    /// wgpu device request failed.
    #[error("device request failed: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),

    /// Surface is incompatible with the chosen adapter.
    #[error("surface is incompatible with adapter")]
    IncompatibleSurface,

    /// Surface texture could not be acquired (e.g. window minimised).
    #[error("failed to acquire surface texture: {0}")]
    SurfaceAcquire(#[from] wgpu::SurfaceError),

    /// A frame operation was called in the wrong order.
    #[error("frame state error: {0}")]
    FrameState(&'static str),
}
