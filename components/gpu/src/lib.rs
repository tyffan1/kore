//! GPU rendering component for Kore browser.
//!
//! Provides a display list abstraction and a wgpu-based renderer that
//! consumes display lists and draws them to a [`wgpu::Surface`]. The
//! renderer also supports an offscreen mode used by the dedicated GPU
//! process, plus a [`Blitter`] that the browser process uses to present
//! GPU-process frames on the window surface.

mod atlas;
mod blitter;
mod display_list;
mod error;
mod pipeline;
mod renderer;
mod vertex;

#[cfg(test)]
mod tests;

pub use atlas::TextureAtlas;
pub use blitter::Blitter;
pub use display_list::{ClipRect, Color, DisplayCommand, DisplayList, DrawCircle, DrawImage, DrawRect, DrawText, GpuImage};
pub use error::GpuError;
pub use pipeline::{CirclePipeline, ImagePipeline, RectPipeline, TextPipeline};
pub use renderer::{FrameRenderer, FrameResult, FrameTarget, OffscreenTarget, Renderer, RendererConfig};
pub use vertex::{circle_quad_vertices, rect_vertices, text_quad_vertices, CircleVertex, TextVertex, Vertex, RECT_INDICES};
