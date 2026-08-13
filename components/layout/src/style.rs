use crate::BoxEdges;
use kore_css::{CascadedProperty, CssColor};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Display {
    Block,
    Inline,
    InlineBlock,
    Flex,
    Grid,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GridTrack {
    Fixed(f32),
    Fraction(f32),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontWeight {
    Normal,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontStyle {
    Normal,
    Italic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputedStyle {
    pub display: Display,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub margin: BoxEdges,
    pub border: BoxEdges,
    pub padding: BoxEdges,
    pub position: Position,
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
    pub grid_columns: Vec<GridTrack>,
    pub grid_rows: Vec<GridTrack>,
    pub grid_auto_rows: Option<f32>,
    pub column_gap: f32,
    pub row_gap: f32,
    pub grid_column_start: Option<u16>,
    pub grid_column_span: u16,
    pub grid_row_start: Option<u16>,
    pub grid_row_span: u16,
    pub background_color: Option<CssColor>,
    pub color: Option<CssColor>,
    pub font_size: Option<f32>,
    pub font_weight: FontWeight,
    pub font_style: FontStyle,
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
            position: Position::Static,
            top: None,
            right: None,
            bottom: None,
            left: None,
            grid_columns: Vec::new(),
            grid_rows: Vec::new(),
            grid_auto_rows: None,
            column_gap: 0.0,
            row_gap: 0.0,
            grid_column_start: None,
            grid_column_span: 1,
            grid_row_start: None,
            grid_row_span: 1,
            background_color: None,
            color: None,
            font_size: None,
            font_weight: FontWeight::Normal,
            font_style: FontStyle::Normal,
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
        style.background_color = map.get("background-color").and_then(|v| CssColor::parse(v));
        style.color = map.get("color").and_then(|v| CssColor::parse(v));
        style.font_size = map.get("font-size").and_then(|v| parse_length(v));

        if let Some(value) = map.get("font-weight") {
            style.font_weight = match *value {
                "bold" | "700" | "800" | "900" => FontWeight::Bold,
                _ => FontWeight::Normal,
            };
        }
        if let Some(value) = map.get("font-style") {
            style.font_style = match *value {
                "italic" | "oblique" => FontStyle::Italic,
                _ => FontStyle::Normal,
            };
        }

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

        if let Some(value) = map.get("position") {
            style.position = parse_position(value);
        }
        style.top = map.get("top").and_then(|value| parse_length(value));
        style.right = map.get("right").and_then(|value| parse_length(value));
        style.bottom = map.get("bottom").and_then(|value| parse_length(value));
        style.left = map.get("left").and_then(|value| parse_length(value));

        if let Some(value) = map.get("grid-template-columns") {
            style.grid_columns = parse_grid_tracks(value);
        }
        if let Some(value) = map.get("grid-template-rows") {
            style.grid_rows = parse_grid_tracks(value);
        }
        style.grid_auto_rows = map.get("grid-auto-rows").and_then(|value| parse_length(value));
        if let Some(value) = map.get("column-gap") {
            style.column_gap = parse_length(value).unwrap_or(0.0);
        }
        if let Some(value) = map.get("row-gap") {
            style.row_gap = parse_length(value).unwrap_or(0.0);
        }
        if let Some(value) = map.get("gap") {
            let mut parts = value.split_whitespace().filter_map(parse_length);
            if let Some(first) = parts.next() {
                style.row_gap = first;
                style.column_gap = parts.next().unwrap_or(first);
            }
        }
        if let Some(value) = map.get("grid-column") {
            let (start, span) = parse_grid_placement(value);
            style.grid_column_start = start;
            style.grid_column_span = span;
        }
        if let Some(value) = map.get("grid-row") {
            let (start, span) = parse_grid_placement(value);
            style.grid_row_start = start;
            style.grid_row_span = span;
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
        "grid" | "inline-grid" => Display::Grid,
        _ => Display::Block,
    }
}

fn parse_position(value: &str) -> Position {
    match value {
        "relative" => Position::Relative,
        "absolute" => Position::Absolute,
        "fixed" => Position::Fixed,
        "sticky" => Position::Sticky,
        _ => Position::Static,
    }
}

/// Parse `grid-template-columns`/`grid-template-rows` values like
/// `"100px 1fr"` or `"repeat(2, 50px 1fr)"` into track definitions.
fn parse_grid_tracks(value: &str) -> Vec<GridTrack> {
    split_grid_value(value)
        .into_iter()
        .filter_map(|token| parse_grid_track(&token))
        .collect()
}

fn parse_grid_track(token: &str) -> Option<GridTrack> {
    let token = token.trim();
    if token.is_empty() || token == "auto" {
        return None;
    }
    if let Some(fraction) = token.strip_suffix("fr") {
        return fraction.trim().parse::<f32>().ok().map(GridTrack::Fraction);
    }
    parse_length(token).map(GridTrack::Fixed)
}

/// Split a track list into top-level tokens, keeping `repeat(...)` groups intact.
fn split_grid_value(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in value.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ch if ch.is_ascii_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            ch => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    let mut expanded = Vec::new();
    for token in tokens {
        if let Some((count, inner)) = parse_repeat(&token) {
            let inner_tracks = split_grid_value(inner);
            for _ in 0..count {
                expanded.extend(inner_tracks.clone());
            }
        } else {
            expanded.push(token);
        }
    }
    expanded
}

/// Parse `repeat(N, <tracks>)` into `(count, inner_tracks)`.
fn parse_repeat(token: &str) -> Option<(usize, &str)> {
    let inner = token.strip_prefix("repeat(")?.strip_suffix(')')?;
    let (count_str, tracks) = inner.split_once(',')?;
    let count = count_str.trim().parse::<usize>().ok()?;
    Some((count, tracks.trim()))
}

/// Parse `grid-column`/`grid-row` values like `"span 2"`, `"2"` or `"2 / span 3"`
/// into `(start_index, span)`. Line numbers are converted to zero-based cell indices.
fn parse_grid_placement(value: &str) -> (Option<u16>, u16) {
    let mut start = None;
    let mut span = 1u16;
    for part in value.split('/') {
        let tokens = part.split_whitespace().collect::<Vec<_>>();
        if tokens.first() == Some(&"span") {
            if let Some(n) = tokens.get(1).and_then(|t| t.parse::<u16>().ok()) {
                span = n.max(1);
            }
        } else if let Some(n) = tokens.first().and_then(|t| t.parse::<u16>().ok()) {
            start = Some(n.saturating_sub(1));
        }
    }
    (start, span)
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
