use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CssColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl CssColor {
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };

    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if let Some(hex) = input.strip_prefix('#') {
            return parse_hex(hex);
        }
        if input.starts_with("rgb") {
            return parse_rgb_function(input);
        }
        parse_named_color(input)
    }
}

fn parse_hex(hex: &str) -> Option<CssColor> {
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            Some(CssColor::from_rgba(r, g, b, 255))
        }
        4 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            let a = u8::from_str_radix(&hex[3..4], 16).ok()? * 17;
            Some(CssColor::from_rgba(r, g, b, a))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(CssColor::from_rgba(r, g, b, 255))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(CssColor::from_rgba(r, g, b, a))
        }
        _ => None,
    }
}

fn parse_rgb_function(input: &str) -> Option<CssColor> {
    let inner = input
        .strip_prefix("rgba")
        .or(input.strip_prefix("rgb"))?;
    let inner = inner.trim().strip_prefix('(')?;
    let inner = inner.trim().strip_suffix(')')?;
    let parts: Vec<&str> = inner.splitn(4, ',').map(|s| s.trim()).collect();
    match parts.len() {
        3 => {
            let r = parts[0].parse().ok()?;
            let g = parts[1].parse().ok()?;
            let b = parts[2].parse().ok()?;
            Some(CssColor::from_rgba(r, g, b, 255))
        }
        4 => {
            let r = parts[0].parse().ok()?;
            let g = parts[1].parse().ok()?;
            let b = parts[2].parse().ok()?;
            let a = (parts[3].parse::<f32>().ok()? * 255.0).round() as u8;
            Some(CssColor::from_rgba(r, g, b, a))
        }
        _ => None,
    }
}

fn parse_named_color(name: &str) -> Option<CssColor> {
    Some(match name.to_ascii_lowercase().as_str() {
        "transparent" => CssColor::TRANSPARENT,
        "black" => CssColor::BLACK,
        "white" => CssColor::WHITE,
        "red" => CssColor::from_rgba(255, 0, 0, 255),
        "green" => CssColor::from_rgba(0, 128, 0, 255),
        "blue" => CssColor::from_rgba(0, 0, 255, 255),
        "yellow" => CssColor::from_rgba(255, 255, 0, 255),
        "cyan" | "aqua" => CssColor::from_rgba(0, 255, 255, 255),
        "magenta" | "fuchsia" => CssColor::from_rgba(255, 0, 255, 255),
        "gray" | "grey" => CssColor::from_rgba(128, 128, 128, 255),
        "silver" => CssColor::from_rgba(192, 192, 192, 255),
        "maroon" => CssColor::from_rgba(128, 0, 0, 255),
        "purple" => CssColor::from_rgba(128, 0, 128, 255),
        "navy" => CssColor::from_rgba(0, 0, 128, 255),
        "olive" => CssColor::from_rgba(128, 128, 0, 255),
        "orange" => CssColor::from_rgba(255, 165, 0, 255),
        "pink" => CssColor::from_rgba(255, 192, 203, 255),
        "brown" => CssColor::from_rgba(165, 42, 42, 255),
        "lime" => CssColor::from_rgba(0, 255, 0, 255),
        "teal" => CssColor::from_rgba(0, 128, 128, 255),
        "indigo" => CssColor::from_rgba(75, 0, 130, 255),
        "violet" => CssColor::from_rgba(238, 130, 238, 255),
        "coral" => CssColor::from_rgba(255, 127, 80, 255),
        "tomato" => CssColor::from_rgba(255, 99, 71, 255),
        "salmon" => CssColor::from_rgba(250, 128, 114, 255),
        "gold" => CssColor::from_rgba(255, 215, 0, 255),
        "darkgray" | "darkgrey" => CssColor::from_rgba(169, 169, 169, 255),
        "lightgray" | "lightgrey" => CssColor::from_rgba(211, 211, 211, 255),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_6() {
        let c = CssColor::parse("#ff8800").unwrap();
        assert_eq!(c, CssColor::from_rgba(255, 136, 0, 255));
    }

    #[test]
    fn parse_hex_3() {
        let c = CssColor::parse("#f80").unwrap();
        assert_eq!(c, CssColor::from_rgba(255, 136, 0, 255));
    }

    #[test]
    fn parse_hex_8_with_alpha() {
        let c = CssColor::parse("#ff880080").unwrap();
        assert_eq!(c, CssColor::from_rgba(255, 136, 0, 128));
    }

    #[test]
    fn parse_rgb() {
        let c = CssColor::parse("rgb(100, 200, 50)").unwrap();
        assert_eq!(c, CssColor::from_rgba(100, 200, 50, 255));
    }

    #[test]
    fn parse_rgba() {
        let c = CssColor::parse("rgba(100, 200, 50, 0.5)").unwrap();
        assert_eq!(c, CssColor::from_rgba(100, 200, 50, 128));
    }

    #[test]
    fn parse_named_red() {
        let c = CssColor::parse("red").unwrap();
        assert_eq!(c, CssColor::from_rgba(255, 0, 0, 255));
    }

    #[test]
    fn parse_transparent() {
        let c = CssColor::parse("transparent").unwrap();
        assert_eq!(c, CssColor::TRANSPARENT);
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert!(CssColor::parse("not-a-color").is_none());
        assert!(CssColor::parse("#xyz").is_none());
    }
}
