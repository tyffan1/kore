//! High-level browser tab / session controller for Kore.

mod app;
mod error;
mod gpu;
mod network;
mod renderer;
mod session;
mod tab;

pub use app::BrowserApp;
pub use error::BrowserError;
pub use gpu::GpuProcess;
pub use network::NetworkProcess;
pub use renderer::RendererProcess;
pub use session::SessionManager;
pub use tab::{Tab, TabManager};
