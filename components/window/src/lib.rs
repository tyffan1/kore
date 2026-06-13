//! Windowing system integration for Kore.
//!
//! Bridges winit window creation with wgpu rendering surfaces and
//! translates raw platform input into [`InputEvent`] values.

mod builder;
mod error;
mod event;
mod event_loop;
mod handle;

pub use builder::{WindowBuilder, WindowConfig};
pub use error::WindowError;
pub use event::{InputEvent, Key, Modifiers, MouseButton};
pub use event_loop::{AppEvent, EventLoop};
pub use handle::WindowHandle;
