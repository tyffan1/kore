use crate::cache::{FontCache, FontDescription, FontId};
use crate::shaper::TextShaper;

/// demo.ttf from ttf-parser's test suite — a minimal valid TrueType font.
const DEMO_TTF: &[u8] = include_bytes!("demo.ttf");

fn make_cache() -> (FontCache, FontId) {
    let mut cache = FontCache::new();
    let desc = FontDescription::new("TestSans", false, false);
    let font_id = cache
        .load_font_bytes(DEMO_TTF, desc)
        .expect("demo.ttf should be parseable");
    (cache, font_id)
}

#[test]
fn font_cache_created_empty() {
    let cache = FontCache::new();
    assert_eq!(cache.font_count(), 0);
}

#[test]
fn font_loads_successfully() {
    let (_, _) = make_cache();
}

#[test]
fn font_count_after_load() {
    let (cache, _) = make_cache();
    assert_eq!(cache.font_count(), 1);
}

#[test]
fn lookup_glyph_returns_index() {
    let (cache, font_id) = make_cache();
    let idx = cache.lookup_glyph(font_id, 'A');
    assert!(idx.is_some(), "should find glyph index for 'A'");
}

#[test]
fn lookup_glyph_returns_some_for_existing_char() {
    let (cache, font_id) = make_cache();
    let idx_a = cache.lookup_glyph(font_id, 'A');
    let idx_a2 = cache.lookup_glyph(font_id, 'A');
    assert_eq!(idx_a, idx_a2, "same char should return same index");
}

#[test]
fn line_metrics_are_positive() {
    let (cache, font_id) = make_cache();
    let metrics = cache.line_metrics(font_id, 16.0);
    assert!(metrics.is_some(), "line metrics should be available");
    if let Some(lm) = metrics {
        assert!(lm.ascent > 0.0, "ascent should be positive");
        assert!(lm.line_gap >= 0.0, "line_gap should be >= 0");
    }
}

#[test]
fn find_font_exact_match() {
    let mut cache = FontCache::new();
    let desc = FontDescription::new("MyFont", false, false);
    let _ = cache.load_font_bytes(DEMO_TTF, desc).unwrap();
    let found = cache.find_font("MyFont", false, false);
    assert!(found.is_some(), "should find exact match");
}

#[test]
fn find_font_fallback_to_any() {
    let mut cache = FontCache::new();
    let desc = FontDescription::new("MyFont", false, false);
    let _ = cache.load_font_bytes(DEMO_TTF, desc).unwrap();
    let found = cache.find_font("OtherFont", false, false);
    assert!(found.is_some(), "should fall back to first font");
}

#[test]
fn rasterize_glyph_caches_result() {
    let (mut cache, font_id) = make_cache();
    let glyph = cache.rasterize_glyph(font_id, 'A', 16.0);
    assert!(glyph.is_some(), "should rasterize 'A' at 16px");
    if let Some(g) = glyph {
        assert!(g.advance_width > 0.0, "advance width should be positive");
    }
    let glyph2 = cache.rasterize_glyph(font_id, 'A', 16.0);
    assert!(glyph2.is_some());
}

#[test]
fn rasterize_at_different_sizes() {
    let (mut cache, font_id) = make_cache();
    let small = cache.rasterize_glyph(font_id, 'A', 10.0).cloned();
    let large = cache.rasterize_glyph(font_id, 'A', 20.0).cloned();
    assert!(small.is_some());
    assert!(large.is_some());
    if let (Some(s), Some(l)) = (small, large) {
        assert!(
            l.advance_width > s.advance_width,
            "larger size should have larger advance width"
        );
    }
}

#[test]
fn cached_glyph_returns_none_before_rasterize() {
    let (cache, font_id) = make_cache();
    let cached = cache.cached_glyph(font_id, 'A', 16.0);
    assert!(cached.is_none(), "should not be cached before first access");
}

#[test]
fn cached_glyph_returns_some_after_rasterize() {
    let (mut cache, font_id) = make_cache();
    let _ = cache.rasterize_glyph(font_id, 'A', 16.0);
    let cached = cache.cached_glyph(font_id, 'A', 16.0);
    assert!(cached.is_some(), "should be cached after rasterize");
}

#[test]
fn multiple_fonts_can_be_loaded() {
    let mut cache = FontCache::new();
    let d1 = FontDescription::new("FontA", false, false);
    let d2 = FontDescription::new("FontB", true, true);
    let id1 = cache.load_font_bytes(DEMO_TTF, d1).unwrap();
    let id2 = cache.load_font_bytes(DEMO_TTF, d2).unwrap();
    assert_eq!(cache.font_count(), 2);
    assert_ne!(id1, id2);
}

#[test]
fn font_description_matches() {
    let desc = FontDescription::new("sans", true, false);
    assert!(desc.matches("sans", true, false));
    assert!(!desc.matches("sans", false, false));
    assert!(!desc.matches("serif", true, false));
}

#[test]
fn text_shaper_shape_line_returns_glyphs() {
    let (mut cache, font_id) = make_cache();
    let shaper = TextShaper::new();
    let (glyphs, width) = shaper.shape_line(&mut cache, font_id, "A", 16.0, 0.0, 100.0);
    assert!(!glyphs.is_empty(), "should produce at least one glyph");
    assert!(width > 0.0, "line width should be positive");
}

#[test]
fn text_shaper_shape_multiple_chars() {
    let (mut cache, font_id) = make_cache();
    let shaper = TextShaper::new();
    let (glyphs, width) = shaper.shape_line(&mut cache, font_id, "AB", 16.0, 0.0, 100.0);
    assert_eq!(glyphs.len(), 2, "should produce two glyphs");
    assert!(glyphs[1].x > glyphs[0].x, "second glyph should be after first");
    assert!(width > 0.0);
}

#[test]
fn text_shaper_shape_wrapped_returns_lines() {
    let (mut cache, font_id) = make_cache();
    let shaper = TextShaper::new();
    let lines = shaper.shape_wrapped(&mut cache, font_id, "A A", 16.0, 500.0);
    assert_eq!(lines.len(), 1, "short text should fit on one line");
    assert!(!lines[0].glyphs.is_empty());
}

#[test]
fn text_shaper_wraps_long_text() {
    let (mut cache, font_id) = make_cache();
    let shaper = TextShaper::new();
    let lines = shaper.shape_wrapped(&mut cache, font_id, "AA AA", 16.0, 1.0);
    assert!(lines.len() >= 2, "should wrap into multiple lines");
}

#[test]
fn text_shaper_empty_string() {
    let (mut cache, font_id) = make_cache();
    let shaper = TextShaper::new();
    let lines = shaper.shape_wrapped(&mut cache, font_id, "", 16.0, 500.0);
    assert!(lines.is_empty(), "empty string should produce no lines");
}

#[test]
fn glyph_metrics_have_valid_dimensions() {
    let (mut cache, font_id) = make_cache();
    let glyph = cache.rasterize_glyph(font_id, 'A', 16.0);
    assert!(glyph.is_some());
    if let Some(g) = glyph {
        assert!(
            g.width <= 32 || g.height <= 32,
            "glyph should not be excessively large at 16px"
        );
    }
}
