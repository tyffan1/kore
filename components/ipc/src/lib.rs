//! Typed IPC foundation for Kore processes.

mod channel;
mod message;
mod transport;
pub mod wire;

pub use channel::{split_transport, IpcError, Receiver, Sender, MAX_FRAME_BYTES};
pub use message::{
    FrameRenderCommand, IpcMessage, IpcPayload, JsEvalRequest, JsEvalResult, MessageId, PageLoaded,
    ProcessId, RenderFrame, TabClosed, TabCreated,
};
pub use transport::{PlatformAddress, PlatformListener, PlatformStream, PlatformTransport};
pub use wire::{
    ClipRect, Color, DisplayCommand, DisplayList, DrawCircle, DrawImage, DrawRect, DrawText,
    FetchRequest, FetchResponse, GpuImage, Method,
};
