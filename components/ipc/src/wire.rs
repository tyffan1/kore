//! Wire-level data types shared across process boundaries.
//!
//! These types are serialized with bincode inside [`crate::IpcMessage`]
//! payloads, so they must live in `kore-ipc` itself — downstream crates
//! (`kore-net`, `kore-gpu`) re-export them so the public API of those
//! crates is unchanged, and depending on `kore-ipc` from either crate
//! does not create a dependency cycle.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use url::Url;

// ──────────────────────────── networking ────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Method {
    Get,
    Head,
    Post,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchRequest {
    pub url: Url,
    pub method: Method,
    pub body: Option<Bytes>,
    pub headers: Vec<(String, String)>,
    /// Registrable domain of the top-level document, when this request is a
    /// subresource. Used for third-party cookie blocking (ETP). `None` for
    /// main-frame navigations.
    pub top_level: Option<String>,
}

impl FetchRequest {
    pub fn get(url: &str) -> Result<Self, url::ParseError> {
        Ok(Self {
            url: Url::parse(url)?,
            method: Method::Get,
            body: None,
            headers: Vec::new(),
            top_level: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchResponse {
    pub status: u16,
    pub final_url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
}

// ──────────────────────────── display list ────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Color,
    pub opacity: f32,
    pub translate: (f32, f32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawText {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub font_size: f32,
    pub color: Color,
    pub font_family: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub opacity: f32,
    pub translate: (f32, f32),
}

/// A rendered frame produced by the GPU process: raw RGBA pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawImage {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub atlas_id: u32,
    pub image: GpuImage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawCircle {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ClipRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ClipRect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }

    pub fn intersects(&self, other: &ClipRect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DisplayCommand {
    Rect(DrawRect),
    Text(DrawText),
    Image(DrawImage),
    Circle(DrawCircle),
    PushClip(ClipRect),
    PopClip,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DisplayList {
    commands: Vec<DisplayCommand>,
}

impl DisplayList {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    pub fn push(&mut self, cmd: DisplayCommand) {
        self.commands.push(cmd);
    }

    pub fn push_rect(&mut self, rect: DrawRect) {
        self.commands.push(DisplayCommand::Rect(rect));
    }

    pub fn push_text(&mut self, text: DrawText) {
        self.commands.push(DisplayCommand::Text(text));
    }

    pub fn push_image(&mut self, image: DrawImage) {
        self.commands.push(DisplayCommand::Image(image));
    }

    pub fn push_circle(&mut self, circle: DrawCircle) {
        self.commands.push(DisplayCommand::Circle(circle));
    }

    pub fn push_clip(&mut self, clip: ClipRect) {
        self.commands.push(DisplayCommand::PushClip(clip));
    }

    pub fn pop_clip(&mut self) {
        self.commands.push(DisplayCommand::PopClip);
    }

    /// Append every command from `other`, shifted by `(dx, dy)`. Used to
    /// embed a nested frame (e.g. an `<iframe>`) into a parent display list.
    pub fn merge_translated(&mut self, other: &DisplayList, dx: f32, dy: f32) {
        for cmd in &other.commands {
            self.commands.push(match cmd {
                DisplayCommand::Rect(r) => DisplayCommand::Rect(DrawRect {
                    x: r.x + dx,
                    y: r.y + dy,
                    translate: r.translate,
                    ..r.clone()
                }),
                DisplayCommand::Text(t) => DisplayCommand::Text(DrawText {
                    x: t.x + dx,
                    y: t.y + dy,
                    translate: t.translate,
                    ..t.clone()
                }),
                DisplayCommand::Image(im) => DisplayCommand::Image(DrawImage {
                    x: im.x + dx,
                    y: im.y + dy,
                    ..im.clone()
                }),
                DisplayCommand::Circle(c) => DisplayCommand::Circle(DrawCircle {
                    cx: c.cx + dx,
                    cy: c.cy + dy,
                    ..c.clone()
                }),
                DisplayCommand::PushClip(clip) => DisplayCommand::PushClip(ClipRect {
                    x: clip.x + dx,
                    y: clip.y + dy,
                    ..*clip
                }),
                DisplayCommand::PopClip => DisplayCommand::PopClip,
            });
        }
    }

    pub fn commands(&self) -> &[DisplayCommand] {
        &self.commands
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_translated_shifts_every_command_kind() {
        let mut inner = DisplayList::new();
        inner.push_rect(DrawRect {
            x: 10.0,
            y: 10.0,
            width: 50.0,
            height: 50.0,
            color: Color::BLACK,
            opacity: 1.0,
            translate: (0.0, 0.0),
        });
        inner.push_text(DrawText {
            x: 10.0,
            y: 10.0,
            text: "hello".to_string(),
            font_size: 12.0,
            color: Color::BLACK,
            font_family: None,
            bold: false,
            italic: false,
            opacity: 1.0,
            translate: (0.0, 0.0),
        });
        inner.push_image(DrawImage {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
            atlas_id: 0,
            image: GpuImage {
                width: 0,
                height: 0,
                pixels: Vec::new(),
            },
        });
        inner.push_circle(DrawCircle {
            cx: 10.0,
            cy: 10.0,
            radius: 5.0,
            color: Color::BLACK,
        });
        inner.push_clip(ClipRect {
            x: 10.0,
            y: 10.0,
            width: 30.0,
            height: 30.0,
        });
        inner.pop_clip();

        let mut outer = DisplayList::new();
        outer.merge_translated(&inner, 100.0, 50.0);

        assert_eq!(outer.len(), 6);
        let cmds = outer.commands();
        match &cmds[0] {
            DisplayCommand::Rect(r) => {
                assert_eq!(r.x, 110.0);
                assert_eq!(r.y, 60.0);
                assert_eq!(r.translate, (0.0, 0.0));
            }
            _ => panic!("expected Rect"),
        }
        match &cmds[1] {
            DisplayCommand::Text(t) => {
                assert_eq!(t.x, 110.0);
                assert_eq!(t.y, 60.0);
            }
            _ => panic!("expected Text"),
        }
        match &cmds[2] {
            DisplayCommand::Image(im) => {
                assert_eq!(im.x, 110.0);
                assert_eq!(im.y, 60.0);
            }
            _ => panic!("expected Image"),
        }
        match &cmds[3] {
            DisplayCommand::Circle(c) => {
                assert_eq!(c.cx, 110.0);
                assert_eq!(c.cy, 60.0);
            }
            _ => panic!("expected Circle"),
        }
        match &cmds[4] {
            DisplayCommand::PushClip(clip) => {
                assert_eq!(clip.x, 110.0);
                assert_eq!(clip.y, 60.0);
                assert_eq!(clip.width, 30.0);
            }
            _ => panic!("expected PushClip"),
        }
        assert!(matches!(&cmds[5], DisplayCommand::PopClip));
    }

    #[test]
    fn merge_translated_leaves_source_untouched() {
        let mut inner = DisplayList::new();
        inner.push_rect(DrawRect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
            color: Color::BLACK,
            opacity: 1.0,
            translate: (0.0, 0.0),
        });
        let mut outer = DisplayList::new();
        outer.merge_translated(&inner, 10.0, 20.0);
        assert_eq!(inner.len(), 1);
        match &inner.commands()[0] {
            DisplayCommand::Rect(r) => {
                assert_eq!(r.x, 1.0);
                assert_eq!(r.y, 2.0);
            }
            _ => panic!("expected Rect"),
        }
    }
}
