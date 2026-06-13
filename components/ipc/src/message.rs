use serde::{Deserialize, Serialize};
use url::Url;

pub type MessageId = u64;
pub type ProcessId = u32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcPayload {
    NavigateToUrl { tab_id: u64, url: Url },
    PageLoaded(PageLoaded),
    TabCreated(TabCreated),
    TabClosed(TabClosed),
    RenderFrame(RenderFrame),
    JSEvalRequest(JsEvalRequest),
    JSEvalResult(JsEvalResult),
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
}
