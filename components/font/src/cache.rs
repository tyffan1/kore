use std::collections::HashMap;

use fontdue::{Font, FontSettings};
use thiserror::Error;

/// Unique identifier for a loaded font face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub usize);

/// Describes which font face to select.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontDescription {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
}

impl FontDescription {
    pub fn new(family: &str, bold: bool, italic: bool) -> Self {
        Self {
            family: family.to_string(),
            bold,
            italic,
        }
    }

    /// Match against a family name and style, returning true if this
    /// description is a candidate.
    pub fn matches(&self, family: &str, bold: bool, italic: bool) -> bool {
        self.family.eq_ignore_ascii_case(family) && self.bold == bold && self.italic == italic
    }
}

/// A rasterized glyph stored in the font cache.
#[derive(Debug, Clone)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    pub x_offset: i32,
    pub y_offset: i32,
    pub advance_width: f32,
    pub pixels: Vec<u8>,
}

/// Errors that can occur during font loading.
#[derive(Debug, Error)]
pub enum FontError {
    #[error("failed to parse font data: {0}")]
    Parse(String),
    #[error("font not found: {0:?}")]
    NotFound(FontDescription),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    font_id: FontId,
    ch: char,
    px_size: u32,
}

/// A cache of loaded fonts and their rasterized glyphs.
#[derive(Debug)]
pub struct FontCache {
    fonts: Vec<Font>,
    descriptions: Vec<FontDescription>,
    glyph_cache: HashMap<GlyphKey, GlyphBitmap>,
}

impl FontCache {
    pub fn new() -> Self {
        Self {
            fonts: Vec::new(),
            descriptions: Vec::new(),
            glyph_cache: HashMap::new(),
        }
    }

    /// Load a font from raw TTF bytes.
    pub fn load_font_bytes(
        &mut self,
        data: &[u8],
        desc: FontDescription,
    ) -> Result<FontId, FontError> {
        let index = self.fonts.len();
        let settings = FontSettings {
            scale: 100.0,
            ..FontSettings::default()
        };
        let font =
            Font::from_bytes(data, settings).map_err(|e| FontError::Parse(e.to_string()))?;
        self.fonts.push(font);
        self.descriptions.push(desc);
        Ok(FontId(index))
    }

    /// Number of loaded fonts.
    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }

    /// Look up a glyph index for a character in a font.
    pub fn lookup_glyph(&self, font_id: FontId, ch: char) -> Option<u16> {
        self.fonts.get(font_id.0).map(|font| font.lookup_glyph_index(ch))
    }

    /// Get horizontal line metrics for a font at a given pixel size.
    pub fn line_metrics(&self, font_id: FontId, size: f32) -> Option<fontdue::LineMetrics> {
        self.fonts.get(font_id.0).and_then(|font| font.horizontal_line_metrics(size))
    }

    /// Find the best matching font for a given description.
    pub fn find_font(&self, family: &str, bold: bool, italic: bool) -> Option<FontId> {
        for (i, desc) in self.descriptions.iter().enumerate() {
            if desc.matches(family, bold, italic) {
                return Some(FontId(i));
            }
        }
        for (i, desc) in self.descriptions.iter().enumerate() {
            if desc.family.eq_ignore_ascii_case(family) {
                return Some(FontId(i));
            }
        }
        self.fonts.first().map(|_| FontId(0))
    }

    /// Rasterize a glyph and return the bitmap, caching the result.
    pub fn rasterize_glyph(
        &mut self,
        font_id: FontId,
        ch: char,
        size: f32,
    ) -> Option<&GlyphBitmap> {
        let font = self.fonts.get(font_id.0)?;
        let key = GlyphKey {
            font_id,
            ch,
            px_size: size.to_bits(),
        };

        if !self.glyph_cache.contains_key(&key) {
            let (metrics, pixels) = font.rasterize(ch, size);
            let bitmap = GlyphBitmap {
                width: metrics.width as u32,
                height: metrics.height as u32,
                x_offset: metrics.xmin,
                y_offset: metrics.ymin,
                advance_width: metrics.advance_width,
                pixels,
            };
            self.glyph_cache.insert(key, bitmap);
        }

        self.glyph_cache.get(&key)
    }

    /// Get a cached glyph bitmap without rasterizing.
    pub fn cached_glyph(&self, font_id: FontId, ch: char, size: f32) -> Option<&GlyphBitmap> {
        let key = GlyphKey {
            font_id,
            ch,
            px_size: size.to_bits(),
        };
        self.glyph_cache.get(&key)
    }

    /// Get the font description for a font ID.
    pub fn font_description(&self, font_id: FontId) -> Option<&FontDescription> {
        self.descriptions.get(font_id.0)
    }

    /// Access the underlying fontdue font for direct use (e.g., measurement).
    pub fn raw_font(&self, font_id: FontId) -> Option<&Font> {
        self.fonts.get(font_id.0)
    }
}

impl Default for FontCache {
    fn default() -> Self {
        Self::new()
    }
}
