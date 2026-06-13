//! GPU rendering component for Kore browser.
//!
//! Provides a display list abstraction and a wgpu-based renderer that
//! consumes display lists and draws them to a [`wgpu::Surface`].

mod atlas;
mod display_list;
mod error;
mod pipeline;
mod renderer;
mod vertex;

#[cfg(test)]
mod tests;

pub use atlas::TextureAtlas;
pub use display_list::{ClipRect, Color, DisplayCommand, DisplayList, DrawImage, DrawRect, DrawText};
pub use error::GpuError;
pub use renderer::{FrameRenderer, Renderer, RendererConfig};
