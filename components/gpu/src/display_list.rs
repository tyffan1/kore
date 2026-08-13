//! Display list types.
//!
//! The wire-level display list types live in `kore-ipc` (they are
//! serialized into IPC messages and shipped to the GPU process); this
//! module re-exports them so `kore_gpu::display_list::*` keeps working.

pub use kore_ipc::{
    ClipRect, Color, DisplayCommand, DisplayList, DrawCircle, DrawImage, DrawRect, DrawText,
    GpuImage,
};
