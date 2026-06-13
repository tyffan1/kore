//! Typed IPC foundation for Kore processes.

mod channel;
mod message;
mod transport;

pub use channel::{split_transport, IpcError, Receiver, Sender, MAX_FRAME_BYTES};
pub use message::{
    FrameRenderCommand, IpcMessage, IpcPayload, JsEvalRequest, JsEvalResult, MessageId, PageLoaded,
    ProcessId, RenderFrame, TabClosed, TabCreated,
};
pub use transport::{PlatformAddress, PlatformTransport};
