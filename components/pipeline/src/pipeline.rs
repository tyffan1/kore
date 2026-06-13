use kore_css::{parse_stylesheet, CssColor};
use kore_gpu::{Color, DisplayList, DrawRect};
use kore_html::{parse_document, NodeKind};
use kore_layout::{layout_document, LayoutConfig, LayoutTree};
use kore_net::{FetchRequest, HttpClient};
use url::Url;

use crate::error::PipelineError;

const DEFAULT_CSS: &str = r#"
html, body, div, p, h1, h2, h3, h4, h5, h6, ul, ol, li,
header, footer, main, nav, section, article, aside,
figure, figcaption, blockquote, dl, dt, dd, form, table {
    display: block;
}
head, script, style, link, meta, title {
    display: none;
}
"#;

/// Result of a full render pipeline run.
pub struct RenderOutput {
    pub display_list: DisplayList,
    pub title: Option<String>,
}

/// The render pipeline: fetch HTML → parse → find CSS → fetch CSS → cascade → layout → display list.
pub struct Pipeline {
    http_client: HttpClient,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new(HttpClient::default())
    }
}

impl Pipeline {
    pub fn new(http_client: HttpClient) -> Self {
        Self { http_client }
    }

    pub fn http_client(&self) -> &HttpClient {
        &self.http_client
    }

    /// Run the full pipeline: fetch, parse, style, layout, and build a display list.
    pub fn render(&self, url: &Url) -> Result<RenderOutput, PipelineError> {
        let html_str = self.fetch_html(url)?;
        let document = parse_document(&html_str)?;

        let title = page_title(&document);

        let mut stylesheets = vec![DEFAULT_CSS.to_string()];

        for css_url in linked_stylesheets(&document, url) {
            if let Ok(css) = self.fetch_css(&css_url) {
                stylesheets.push(css);
            }
        }

        let combined_css = stylesheets.join("\n");
        let stylesheet = parse_stylesheet(&combined_css)?;

        let (width, height) = (1264.0, 628.0);
        let layout_tree = layout_document(
            &document,
            &stylesheet,
            LayoutConfig {
                viewport_width: width,
                viewport_height: height,
            },
        )?;

        let display_list = build_display_list(&document, &layout_tree);

        Ok(RenderOutput { display_list, title })
    }

    fn fetch_html(&self, url: &Url) -> Result<String, PipelineError> {
        if is_about_blank(url) {
            return Ok(String::new());
        }
        let request = FetchRequest::get(url.as_str())?;
        let response = pollster::block_on(self.http_client.fetch(request))?;
        String::from_utf8(response.body.to_vec()).map_err(|_| PipelineError::InvalidUtf8)
    }

    fn fetch_css(&self, url: &Url) -> Result<String, PipelineError> {
        let request = FetchRequest::get(url.as_str())?;
        let response = pollster::block_on(self.http_client.fetch(request))?;
        String::from_utf8(response.body.to_vec()).map_err(|_| PipelineError::InvalidUtf8)
    }
}

fn is_about_blank(url: &Url) -> bool {
    url.as_str() == "about:blank" || url.as_str() == "about:newtab"
}

/// Extract the page title from a `<title>` element.
pub fn page_title(document: &kore_html::Document) -> Option<String> {
    for node in document.nodes() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag_name.eq_ignore_ascii_case("title") {
                for child_id in &node.children {
                    if let Some(child) = document.node(*child_id) {
                        if let NodeKind::Text(text) = &child.kind {
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                return Some(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Find `<link rel="stylesheet">` elements and resolve their href to absolute URLs.
pub fn linked_stylesheets(document: &kore_html::Document, base: &Url) -> Vec<Url> {
    let mut urls = Vec::new();
    for node in document.nodes() {
        if let NodeKind::Element(el) = &node.kind {
            if el.tag_name.eq_ignore_ascii_case("link") {
                let is_stylesheet = el.attributes.iter().any(|attr| {
                    attr.name.eq_ignore_ascii_case("rel")
                        && attr.value.to_ascii_lowercase() == "stylesheet"
                });
                if !is_stylesheet {
                    continue;
                }
                if let Some(href) = el.attributes.iter().find(|a| a.name.eq_ignore_ascii_case("href")) {
                    if let Ok(url) = base.join(&href.value) {
                        urls.push(url);
                    }
                }
            }
        }
    }
    urls
}

/// Convert a CssColor (kore-css) to a Color (kore-gpu).
fn to_gpu_color(css: CssColor) -> Color {
    Color::from_rgba8(css.r, css.g, css.b, css.a)
}

/// Default background color for an element type.
fn default_bg_color(tag_name: &str) -> Option<Color> {
    match tag_name {
        "html" | "body" => Some(Color::from_rgba8(255, 255, 255, 255)),
        _ => None,
    }
}

/// Build a DisplayList from a LayoutTree and its associated DOM.
///
/// Each layout node with a non-zero rect gets rendered using:
/// - Its `background_color` if set,
/// - A default element background if one exists for its tag,
/// - Otherwise it is skipped (no background).
pub fn build_display_list(document: &kore_html::Document, layout_tree: &LayoutTree) -> DisplayList {
    let mut dl = DisplayList::new();

    for node in &layout_tree.nodes {
        if node.rect.width <= 0.0 || node.rect.height <= 0.0 {
            continue;
        }

        let color = node.style.background_color.map(to_gpu_color).or_else(|| {
            node.dom_node_id.and_then(|dom_id| {
                document.node(dom_id).and_then(|dom_node| {
                    if let NodeKind::Element(el) = &dom_node.kind {
                        default_bg_color(&el.tag_name)
                    } else {
                        None
                    }
                })
            })
        });

        if let Some(color) = color {
            dl.push_rect(DrawRect {
                x: node.rect.x,
                y: node.rect.y,
                width: node.rect.width,
                height: node.rect.height,
                color,
            });
        }
    }

    dl
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_render(html: &str, css: &str) -> (kore_html::Document, LayoutTree, DisplayList) {
        let document = parse_document(html).unwrap();
        let combined = format!("{}\n{}", DEFAULT_CSS, css);
        let stylesheet = parse_stylesheet(&combined).unwrap();
        let layout_tree = layout_document(
            &document,
            &stylesheet,
            LayoutConfig {
                viewport_width: 800.0,
                viewport_height: 600.0,
            },
        )
        .unwrap();
        let dl = build_display_list(&document, &layout_tree);
        (document, layout_tree, dl)
    }

    fn find_rect(dl: &DisplayList, r: u8, g: u8, b: u8) -> Option<&DrawRect> {
        for cmd in dl.commands() {
            if let kore_gpu::DisplayCommand::Rect(rect) = cmd {
                let expected = Color::from_rgba8(r, g, b, 255);
                if (rect.color.r - expected.r).abs() < 1.0 / 255.0
                    && (rect.color.g - expected.g).abs() < 1.0 / 255.0
                    && (rect.color.b - expected.b).abs() < 1.0 / 255.0
                {
                    return Some(rect);
                }
            }
        }
        None
    }

    #[test]
    fn test_page_title_from_html() {
        let doc = parse_document("<html><head><title>Hello World</title></head><body></body></html>").unwrap();
        assert_eq!(page_title(&doc), Some("Hello World".to_string()));
    }

    #[test]
    fn test_page_title_empty_when_no_title() {
        let doc = parse_document("<html><body><p>no title</p></body></html>").unwrap();
        assert_eq!(page_title(&doc), None);
    }

    #[test]
    fn test_linked_stylesheets_found() {
        let html = r#"<html><head><link rel="stylesheet" href="style.css"></head><body></body></html>"#;
        let doc = parse_document(html).unwrap();
        let base = Url::parse("https://example.com/").unwrap();
        let urls = linked_stylesheets(&doc, &base);
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].as_str(), "https://example.com/style.css");
    }

    #[test]
    fn test_linked_stylesheets_ignores_non_css() {
        let html = r#"<html><head><link rel="icon" href="favicon.ico"></head><body></body></html>"#;
        let doc = parse_document(html).unwrap();
        let base = Url::parse("https://example.com/").unwrap();
        let urls = linked_stylesheets(&doc, &base);
        assert!(urls.is_empty());
    }

    #[test]
    fn test_background_color_from_css() {
        let (_, _, dl) = run_render(
            r#"<div id="box">content</div>"#,
            "#box { background-color: #ff0000; width: 100px; height: 50px; }",
        );
        let rect = find_rect(&dl, 255, 0, 0).expect("should have a red rect");
        assert_eq!(rect.width, 100.0);
        assert_eq!(rect.height, 50.0);
    }

    #[test]
    fn test_multiple_colored_divs() {
        let (_, _, dl) = run_render(
            r#"
                <div id="red"></div>
                <div id="blue"></div>
            "#,
            r#"
                #red { background-color: rgb(255,0,0); width: 50px; height: 50px; }
                #blue { background-color: blue; width: 60px; height: 40px; }
            "#,
        );
        assert!(find_rect(&dl, 255, 0, 0).is_some(), "red rect missing");
        assert!(find_rect(&dl, 0, 0, 255).is_some(), "blue rect missing");
    }

    #[test]
    fn test_body_has_white_background_by_default() {
        let (_, _, dl) = run_render(
            r#"<html><body><p>text</p></body></html>"#,
            "",
        );
        let white = find_rect(&dl, 255, 255, 255);
        assert!(white.is_some(), "body should have white background");
    }

    #[test]
    fn test_skip_zero_size_nodes() {
        let (_, _, dl) = run_render(
            r#"<div id="empty"></div>"#,
            "#empty { background-color: red; }", // no width/height → zero-size
        );
        let red_rect = find_rect(&dl, 255, 0, 0);
        assert!(red_rect.is_none(), "zero-size node should be skipped");
    }

    #[test]
    fn test_no_display_list_for_blank() {
        let doc = parse_document("").unwrap();
        let stylesheet = parse_stylesheet(DEFAULT_CSS).unwrap();
        let layout_tree = layout_document(
            &doc,
            &stylesheet,
            LayoutConfig::default(),
        )
        .unwrap();
        let dl = build_display_list(&doc, &layout_tree);
        assert!(dl.is_empty());
    }

    #[test]
    fn test_color_with_named_color() {
        let (_, _, dl) = run_render(
            r#"<div id="g">green</div>"#,
            "#g { background-color: green; width: 50px; height: 50px; }",
        );
        assert!(
            find_rect(&dl, 0, 128, 0).is_some(),
            "named green rect missing"
        );
    }

    #[test]
    fn test_color_with_hex_alpha() {
        let (_, _, dl) = run_render(
            r#"<div id="a">alpha</div>"#,
            "#a { background-color: #ff000080; width: 50px; height: 50px; }",
        );
        for cmd in dl.commands() {
            if let kore_gpu::DisplayCommand::Rect(rect) = cmd {
                assert!((rect.color.a - 0.502).abs() < 0.01, "alpha should be ~0.5");
                return;
            }
        }
        panic!("no rect found");
    }
}
