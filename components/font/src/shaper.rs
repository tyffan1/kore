use crate::cache::{FontCache, FontId};

/// A single shaped glyph ready for layout.
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    pub ch: char,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub advance_width: f32,
}

/// A line of shaped text.
#[derive(Debug, Clone)]
pub struct ShapedLine {
    pub glyphs: Vec<ShapedGlyph>,
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
}

/// Shaper that converts text strings into positioned glyphs.
#[derive(Debug)]
pub struct TextShaper;

impl TextShaper {
    pub fn new() -> Self {
        Self
    }

    /// Shape a single line of text into glyphs with x/y positions.
    pub fn shape_line(
        &self,
        cache: &mut FontCache,
        font_id: FontId,
        text: &str,
        font_size: f32,
        start_x: f32,
        start_y: f32,
    ) -> (Vec<ShapedGlyph>, f32) {
        let mut glyphs = Vec::new();
        let mut cursor_x = start_x;

        for ch in text.chars() {
            let metrics = cache.rasterize_glyph(font_id, ch, font_size);
            if let Some(glyph) = metrics {
                glyphs.push(ShapedGlyph {
                    ch,
                    x: cursor_x + glyph.x_offset as f32,
                    y: start_y - glyph.y_offset as f32 - glyph.height as f32,
                    width: glyph.width as f32,
                    height: glyph.height as f32,
                    advance_width: glyph.advance_width,
                });
                cursor_x += glyph.advance_width;
            }
        }

        let total_width = cursor_x - start_x;
        (glyphs, total_width)
    }

    /// Shape text with word wrapping at max_width.
    pub fn shape_wrapped(
        &self,
        cache: &mut FontCache,
        font_id: FontId,
        text: &str,
        font_size: f32,
        max_width: f32,
    ) -> Vec<ShapedLine> {
        let mut lines: Vec<ShapedLine> = Vec::new();
        let words: Vec<&str> = text.split(' ').collect();
        let space_width = self.advance(cache, font_id, ' ', font_size);

        let mut line_glyphs: Vec<ShapedGlyph> = Vec::new();
        let mut line_width = 0.0_f32;

        for word in words.iter() {
            if word.is_empty() {
                continue;
            }

            let (word_glyphs, word_width) =
                self.shape_line(cache, font_id, word, font_size, 0.0, 0.0);

            let is_first = line_glyphs.is_empty();
            let added_width = if is_first { word_width } else { space_width + word_width };

            if line_width + added_width > max_width && !is_first {
                let line = self.finalize_line(&line_glyphs, font_id, cache, font_size);
                lines.push(line);
                line_glyphs.clear();

                for g in &word_glyphs {
                    line_glyphs.push(g.clone());
                }
                line_width = word_width;
            } else {
                if !is_first {
                    line_glyphs.push(ShapedGlyph {
                        ch: ' ',
                        x: line_width,
                        y: 0.0,
                        width: space_width,
                        height: 0.0,
                        advance_width: space_width,
                    });
                    line_width += space_width;
                }

                for g in &word_glyphs {
                    let mut placed = g.clone();
                    placed.x += line_width;
                    line_glyphs.push(placed);
                }
                line_width += word_width;
            }
        }

        if !line_glyphs.is_empty() {
            let line = self.finalize_line(&line_glyphs, font_id, cache, font_size);
            lines.push(line);
        }

        lines
    }

    fn finalize_line(
        &self,
        glyphs: &[ShapedGlyph],
        font_id: FontId,
        cache: &FontCache,
        font_size: f32,
    ) -> ShapedLine {
        let width = glyphs.last().map_or(0.0, |g| g.x + g.advance_width);
        let line_metrics = cache.line_metrics(font_id, font_size);
        let ascent = line_metrics.map_or(0.0, |lm| lm.ascent);
        let height = line_metrics.map_or(0.0, |lm| lm.ascent - lm.descent + lm.line_gap);

        ShapedLine {
            glyphs: glyphs.to_vec(),
            width,
            height,
            ascent,
        }
    }

    fn advance(&self, cache: &FontCache, font_id: FontId, ch: char, font_size: f32) -> f32 {
        if let Some(font) = cache.raw_font(font_id) {
            let (metrics, _) = font.rasterize(ch, font_size);
            return metrics.advance_width;
        }
        font_size * 0.33
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}
