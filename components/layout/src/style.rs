use crate::BoxEdges;
use kore_css::CascadedProperty;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
    Flex,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JustifyContent {
    FlexStart,
    Center,
    FlexEnd,
    SpaceBetween,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlignItems {
    FlexStart,
    Center,
    FlexEnd,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputedStyle {
    pub display: Display,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub margin: BoxEdges,
    pub border: BoxEdges,
    pub padding: BoxEdges,
    pub flex_direction: FlexDirection,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub flex_wrap: FlexWrap,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Inline,
            width: None,
            height: None,
            margin: BoxEdges::ZERO,
            border: BoxEdges::ZERO,
            padding: BoxEdges::ZERO,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            flex_wrap: FlexWrap::NoWrap,
        }
    }
}

impl ComputedStyle {
    pub fn from_cascade(properties: &[CascadedProperty], default_display: Display) -> Self {
        let mut style = Self {
            display: default_display,
            ..Self::default()
        };
        let map = properties
            .iter()
            .map(|property| (property.property.as_str(), property.value.as_str()))
            .collect::<BTreeMap<_, _>>();

        if let Some(value) = map.get("display") {
            style.display = parse_display(value);
        }
        style.width = map.get("width").and_then(|value| parse_length(value));
        style.height = map.get("height").and_then(|value| parse_length(value));
        style.margin = parse_edges(&map, "margin");
        style.border = parse_edges(&map, "border");
        style.padding = parse_edges(&map, "padding");
        if let Some(value) = map.get("flex-direction") {
            style.flex_direction = match *value {
                "column" | "column-reverse" => FlexDirection::Column,
                _ => FlexDirection::Row,
            };
        }
        if let Some(value) = map.get("justify-content") {
            style.justify_content = match *value {
                "center" => JustifyContent::Center,
                "flex-end" | "end" => JustifyContent::FlexEnd,
                "space-between" => JustifyContent::SpaceBetween,
                _ => JustifyContent::FlexStart,
            };
        }
        if let Some(value) = map.get("align-items") {
            style.align_items = match *value {
                "center" => AlignItems::Center,
                "flex-end" | "end" => AlignItems::FlexEnd,
                "flex-start" | "start" => AlignItems::FlexStart,
                _ => AlignItems::Stretch,
            };
        }
        if let Some(value) = map.get("flex-wrap") {
            style.flex_wrap = match *value {
                "wrap" | "wrap-reverse" => FlexWrap::Wrap,
                _ => FlexWrap::NoWrap,
            };
        }
        style
    }

    pub fn border_box_width(&self, containing_width: f32) -> f32 {
        self.width.unwrap_or(containing_width.max(0.0))
            + self.padding.horizontal()
            + self.border.horizontal()
    }

    pub fn content_width(&self, border_box_width: f32) -> f32 {
        (border_box_width - self.padding.horizontal() - self.border.horizontal()).max(0.0)
    }
}

fn parse_display(value: &str) -> Display {
    match value {
        "none" => Display::None,
        "inline" => Display::Inline,
        "inline-block" => Display::InlineBlock,
        "flex" | "inline-flex" => Display::Flex,
        _ => Display::Block,
    }
}

fn parse_edges(map: &BTreeMap<&str, &str>, prefix: &str) -> BoxEdges {
    let mut edges = map
        .get(prefix)
        .map(|value| parse_edge_shorthand(value))
        .unwrap_or(BoxEdges::ZERO);
    if let Some(value) = map
        .get(format!("{prefix}-top").as_str())
        .and_then(|value| parse_length(value))
    {
        edges.top = value;
    }
    if let Some(value) = map
        .get(format!("{prefix}-right").as_str())
        .and_then(|value| parse_length(value))
    {
        edges.right = value;
    }
    if let Some(value) = map
        .get(format!("{prefix}-bottom").as_str())
        .and_then(|value| parse_length(value))
    {
        edges.bottom = value;
    }
    if let Some(value) = map
        .get(format!("{prefix}-left").as_str())
        .and_then(|value| parse_length(value))
    {
        edges.left = value;
    }
    edges
}

fn parse_edge_shorthand(value: &str) -> BoxEdges {
    let values = value
        .split_whitespace()
        .filter_map(parse_length)
        .collect::<Vec<_>>();
    match values.as_slice() {
        [all] => BoxEdges {
            top: *all,
            right: *all,
            bottom: *all,
            left: *all,
        },
        [vertical, horizontal] => BoxEdges {
            top: *vertical,
            right: *horizontal,
            bottom: *vertical,
            left: *horizontal,
        },
        [top, horizontal, bottom] => BoxEdges {
            top: *top,
            right: *horizontal,
            bottom: *bottom,
            left: *horizontal,
        },
        [top, right, bottom, left, ..] => BoxEdges {
            top: *top,
            right: *right,
            bottom: *bottom,
            left: *left,
        },
        _ => BoxEdges::ZERO,
    }
}

fn parse_length(value: &str) -> Option<f32> {
    let trimmed = value.trim();
    if trimmed == "auto" {
        return None;
    }
    let number = trimmed
        .strip_suffix("px")
        .or_else(|| trimmed.strip_suffix("rem"))
        .unwrap_or(trimmed)
        .split_whitespace()
        .next()
        .unwrap_or_default();
    number.parse::<f32>().ok()
}
