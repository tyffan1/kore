use crate::wire::{DisplayList, FetchRequest, FetchResponse};
use serde::{Deserialize, Serialize};
use url::Url;

pub type MessageId = u64;
pub type ProcessId = u32;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcMessage {
    pub message_id: MessageId,
    pub sender_process_id: ProcessId,
    pub payload: IpcPayload,
}

impl IpcMessage {
    pub fn new(message_id: MessageId, sender_process_id: ProcessId, payload: IpcPayload) -> Self {
        Self {
            message_id,
            sender_process_id,
            payload,
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IpcPayload {
    NavigateToUrl { tab_id: u64, url: Url },
    PageLoaded(PageLoaded),
    TabCreated(TabCreated),
    TabClosed(TabClosed),
    RenderFrame(RenderFrame),
    JSEvalRequest(JsEvalRequest),
    JSEvalResult(JsEvalResult),
    /// A page-load fetch handled by the dedicated network process.
    Fetch { request: FetchRequest },
    /// Response from the network process; the error carries a
    /// human-readable description when the fetch failed.
    FetchResult { response: Result<FetchResponse, String> },
    /// A compositing frame handled by the dedicated GPU process.
    RenderGpuFrame {
        frame_id: u64,
        width: u32,
        height: u32,
        display_list: DisplayList,
    },
    /// Pixels produced by the GPU process (RGBA, row-major).
    GpuFrameRendered {
        frame_id: u64,
        width: u32,
        height: u32,
        /// Raw RGBA bytes; see `GpuImage::pixels` for why `serde_bytes`.
        #[serde(with = "serde_bytes")]
        pixels: Vec<u8>,
    },
    /// The GPU process failed to render a frame.
    GpuFrameFailed { frame_id: u64, error: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageLoaded {
    pub tab_id: u64,
    pub url: Url,
    pub status_code: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabCreated {
    pub tab_id: u64,
    pub initial_url: Option<Url>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabClosed {
    pub tab_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderFrame {
    pub tab_id: u64,
    pub frame_id: u64,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub commands: Vec<FrameRenderCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameRenderCommand {
    Clear {
        color: String,
    },
    Text {
        x: i32,
        y: i32,
        text: String,
    },
    Rect {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        color: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsEvalRequest {
    pub tab_id: u64,
    pub request_id: u64,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsEvalResult {
    pub tab_id: u64,
    pub request_id: u64,
    pub result: Result<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::{Color, DrawRect};

    fn url(input: &str) -> Result<Url, url::ParseError> {
        Url::parse(input)
    }

    fn assert_roundtrip(payload: IpcPayload) -> Result<(), Box<dyn std::error::Error>> {
        let message = IpcMessage::new(42, 7, payload);
        let encoded = message.to_bytes()?;
        let decoded = IpcMessage::from_bytes(&encoded)?;
        assert_eq!(decoded, message);
        Ok(())
    }

    #[test]
    fn roundtrips_navigate_to_url() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::NavigateToUrl {
            tab_id: 1,
            url: url("https://example.com/")?,
        })
    }

    #[test]
    fn roundtrips_page_loaded() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::PageLoaded(PageLoaded {
            tab_id: 1,
            url: url("https://example.com/done")?,
            status_code: 200,
        }))
    }

    #[test]
    fn roundtrips_tab_created() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::TabCreated(TabCreated {
            tab_id: 2,
            initial_url: Some(url("about:blank")?),
        }))
    }

    #[test]
    fn roundtrips_tab_closed() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::TabClosed(TabClosed { tab_id: 2 }))
    }

    #[test]
    fn roundtrips_render_frame() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::RenderFrame(RenderFrame {
            tab_id: 3,
            frame_id: 99,
            viewport_width: 1280,
            viewport_height: 720,
            commands: vec![
                FrameRenderCommand::Clear {
                    color: "#ffffff".to_string(),
                },
                FrameRenderCommand::Rect {
                    x: 12,
                    y: 24,
                    width: 320,
                    height: 180,
                    color: "#0a0a0a".to_string(),
                },
                FrameRenderCommand::Text {
                    x: 16,
                    y: 32,
                    text: "Kore".to_string(),
                },
            ],
        }))
    }

    #[test]
    fn roundtrips_js_eval_request() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::JSEvalRequest(JsEvalRequest {
            tab_id: 4,
            request_id: 11,
            source: "document.title".to_string(),
        }))
    }

    #[test]
    fn roundtrips_js_eval_result() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::JSEvalResult(JsEvalResult {
            tab_id: 4,
            request_id: 11,
            result: Ok("Kore".to_string()),
        }))
    }

    #[test]
    fn roundtrips_fetch_request() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::Fetch {
            request: FetchRequest::get("https://example.com/page")?,
        })
    }

    #[test]
    fn roundtrips_fetch_result_ok() -> Result<(), Box<dyn std::error::Error>> {
        let response = FetchResponse {
            status: 200,
            final_url: url("https://example.com/page")?,
            headers: vec![("content-type".to_string(), "text/html".to_string())],
            body: bytes::Bytes::from_static(b"<html></html>"),
        };
        assert_roundtrip(IpcPayload::FetchResult {
            response: Ok(response),
        })
    }

    #[test]
    fn roundtrips_fetch_result_err() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::FetchResult {
            response: Err("connection refused".to_string()),
        })
    }

    #[test]
    fn roundtrips_render_gpu_frame() -> Result<(), Box<dyn std::error::Error>> {
        let mut display_list = DisplayList::new();
        display_list.push_rect(DrawRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            color: Color::from_rgba8(255, 0, 0, 255),
            opacity: 1.0,
            translate: (0.0, 0.0),
        });
        assert_roundtrip(IpcPayload::RenderGpuFrame {
            frame_id: 7,
            width: 1280,
            height: 720,
            display_list,
        })
    }

    #[test]
    fn roundtrips_gpu_frame_rendered() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::GpuFrameRendered {
            frame_id: 7,
            width: 1280,
            height: 720,
            pixels: vec![0u8; 1280 * 720 * 4],
        })
    }

    #[test]
    fn roundtrips_gpu_frame_failed() -> Result<(), Box<dyn std::error::Error>> {
        assert_roundtrip(IpcPayload::GpuFrameFailed {
            frame_id: 7,
            error: "no adapter".to_string(),
        })
    }
}
