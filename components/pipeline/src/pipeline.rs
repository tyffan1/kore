use kore_css::{cascade_for_element, parse_stylesheet, CssColor, ElementSnapshot};
use kore_gpu::{Color, DisplayList, DrawRect, DrawText};
use kore_html::{parse_document, NodeKind};
use kore_layout::{layout_document, Display, FontStyle, FontWeight, LayoutConfig, LayoutTree};
use kore_net::{FetchRequest, HttpClient};
use url::Url;

use crate::error::PipelineError;

const DEFAULT_CSS: &str = r#"
html { display: block !important; }
body { display: block !important; margin: 8px; font-size: 15px; color: black; }
div { display: block !important; margin: 8px 0; }
p { display: block !important; margin: 16px 0; }
h1 { display: block !important; font-size: 32px; font-weight: bold; margin: 32px 0; }
h2 { display: block !important; font-size: 24px; font-weight: bold; margin: 24px 0; }
h3 { display: block !important; font-size: 18px; font-weight: bold; margin: 20px 0; }
h4 { display: block !important; }
h5 { display: block !important; }
h6 { display: block !important; }
ul { display: block !important; }
ol { display: block !important; }
li { display: block !important; }
header { display: block !important; }
footer { display: block !important; }
main { display: block !important; }
nav { display: block !important; }
section { display: block !important; }
article { display: block !important; }
aside { display: block !important; }
figure { display: block !important; }
figcaption { display: block !important; }
blockquote { display: block !important; }
dl { display: block !important; }
dt { display: block !important; }
dd { display: block !important; }
form { display: block !important; }
table { display: block !important; }
head { display: none !important; }
script { display: none !important; }
style { display: none !important; }
link { display: none !important; }
meta { display: none !important; }
title { display: none !important; }
b { font-weight: bold; }
strong { font-weight: bold; }
i { font-style: italic; }
em { font-style: italic; }
"#;

/// Result of a full render pipeline run.
pub struct RenderOutput {
    pub display_list: DisplayList,
    pub title: Option<String>,
    pub links: Vec<(f32, f32, f32, f32, String)>,
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
    pub async fn render(&self, url: &Url) -> Result<RenderOutput, PipelineError> {
        let html_str = self.fetch_html(url).await?;
        let document = parse_document(&html_str)?;

        if let Some(root_node) = document.node(document.root()) {
            eprintln!("Document root children: {}", root_node.children.len());
            for child_id in &root_node.children {
                if let Some(child) = document.node(*child_id) {
                    match &child.kind {
                        kore_html::NodeKind::Element(el) => {
                            eprintln!("  Root child: <{}> with {} children",
                                el.tag_name, child.children.len());
                        }
                        kore_html::NodeKind::Text(t) => {
                            eprintln!("  Root child: text {:?}", &t[..t.len().min(30)]);
                        }
                        _ => eprintln!("  Root child: other"),
                    }
                }
            }
        }

        let title = page_title(&document);

        let mut stylesheets = vec![DEFAULT_CSS.to_string()];

        for css_url in linked_stylesheets(&document, url) {
            if let Ok(css) = self.fetch_css(&css_url).await {
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

        let display_list = build_display_list_recursive(
            &document,
            &layout_tree,
            &stylesheet,
            width,
        );
        let links = extract_links(&document, &layout_tree);

        eprintln!("HTML length: {}", html_str.len());
        eprintln!("DOM nodes: {}", document.nodes().len());
        eprintln!("Layout nodes: {}", layout_tree.nodes.len());
        eprintln!("Display list commands: {}", display_list.len());
        for cmd in display_list.commands() {
            eprintln!("Command: {:?}", cmd);
        }

        Ok(RenderOutput { display_list, title, links })
    }

    async fn fetch_html(&self, url: &Url) -> Result<String, PipelineError> {
        if is_about_blank(url) {
            return Ok(String::new());
        }
        let request = FetchRequest::get(url.as_str())?;
        let response = self.http_client.fetch(request).await?;
        String::from_utf8(response.body.to_vec()).map_err(|_| PipelineError::InvalidUtf8)
    }

    async fn fetch_css(&self, url: &Url) -> Result<String, PipelineError> {
        let request = FetchRequest::get(url.as_str())?;
        let response = self.http_client.fetch(request).await?;
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

fn parse_display(value: &str) -> Display {
    match value {
        "none" => Display::None,
        "inline" => Display::Inline,
        "inline-block" => Display::InlineBlock,
        "flex" | "inline-flex" => Display::Flex,
        _ => Display::Block,
    }
}

fn default_display_for_tag(tag_name: &str) -> Display {
    match tag_name {
        "html" | "body" | "div" | "p" | "h1" | "h2" | "h3" | "h4"
        | "h5" | "h6" | "ul" | "ol" | "li" | "header" | "footer"
        | "main" | "nav" | "section" | "article" | "aside" | "form"
        | "table" | "tr" | "td" | "th" | "thead" | "tbody" | "tfoot"
        | "figure" | "figcaption" | "blockquote" | "dl" | "dt" | "dd"
            => Display::Block,
        "a" | "b" | "em" | "i" | "label" | "span" | "strong" | "button"
            => Display::Inline,
        "script" | "style" | "template" | "head" | "link" | "meta" | "title"
            => Display::None,
        _ => Display::Block,
    }
}

pub fn build_display_list_recursive(
    document: &kore_html::Document,
    layout_tree: &LayoutTree,
    stylesheet: &kore_css::StyleSheet,
    viewport_width: f32,
) -> DisplayList {
    let mut dl = DisplayList::new();
    let mut cursor_y = 24.0;

    if let Some(root) = document.node(document.root()) {
        for child_id in &root.children {
            traverse_node(*child_id, document, layout_tree, stylesheet, viewport_width, &mut cursor_y, &mut dl);
        }
    }

    dl
}

fn traverse_node(
    dom_id: kore_html::NodeId,
    document: &kore_html::Document,
    layout_tree: &LayoutTree,
    stylesheet: &kore_css::StyleSheet,
    viewport_width: f32,
    cursor_y: &mut f32,
    dl: &mut DisplayList,
) {
    let Some(node) = document.node(dom_id) else { return };
    match &node.kind {
        NodeKind::Element(el) => {
            let snapshot = ElementSnapshot::new(&el.tag_name);
            let properties = cascade_for_element(stylesheet, &snapshot);
            let display = properties
                .iter()
                .find(|p| p.property == "display")
                .map(|p| parse_display(&p.value))
                .unwrap_or_else(|| default_display_for_tag(&el.tag_name));

            if display == Display::None {
                return;
            }

            if let Some(ln) = layout_tree.nodes.iter().find(|n| n.dom_node_id == Some(dom_id)) {
                if ln.rect.width > 0.0 && ln.rect.height > 0.0 {
                    if let Some(color) = ln.style.background_color.map(to_gpu_color).or_else(|| default_bg_color(&el.tag_name)) {
                        dl.push_rect(DrawRect { x: ln.rect.x, y: ln.rect.y, width: ln.rect.width, height: ln.rect.height, color });
                    }
                }
            }

            for child_id in &node.children {
                traverse_node(*child_id, document, layout_tree, stylesheet, viewport_width, cursor_y, dl);
            }
        }
        NodeKind::Text(text) => {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                let font_size = 16.0;
                *cursor_y += font_size * 1.5;
                let x = 10.0;
                dl.push_text(DrawText {
                    x,
                    y: *cursor_y,
                    text: trimmed.to_string(),
                    font_size,
                    color: Color::BLACK,
                    font_family: Some("sans-serif".to_string()),
                    bold: false,
                    italic: false,
                });
            }
        }
        _ => {}
    }
}

/// Extract clickable link regions from the layout tree.
pub fn extract_links(
    document: &kore_html::Document,
    layout_tree: &LayoutTree,
) -> Vec<(f32, f32, f32, f32, String)> {
    let mut links = Vec::new();
    for node in &layout_tree.nodes {
        if node.rect.width <= 0.0 || node.rect.height <= 0.0 {
            continue;
        }
        let Some(dom_id) = node.dom_node_id else { continue };
        let Some(dom_node) = document.node(dom_id) else { continue };
        let NodeKind::Element(el) = &dom_node.kind else { continue };
        if !el.tag_name.eq_ignore_ascii_case("a") {
            continue;
        }
        let Some(href) = el.attributes.iter().find(|a| a.name.eq_ignore_ascii_case("href")) else {
            continue;
        };
        let text_content: String = dom_node
            .children
            .iter()
            .filter_map(|child_id| document.node(*child_id))
            .filter_map(|child| {
                if let NodeKind::Text(t) = &child.kind {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<&str>>()
            .join("");
        let trimmed = text_content.trim();
        if trimmed.is_empty() {
            continue;
        }
        let font_size = node.style.font_size.unwrap_or(16.0);
        let link_w = trimmed.chars().count() as f32 * font_size * 0.6;
        let link_h = font_size * 1.4;
        links.push((node.rect.x, node.rect.y, link_w, link_h, href.value.clone()));
    }
    links
}

/// Build a DisplayList from a LayoutTree and its associated DOM.
pub fn build_display_list(document: &kore_html::Document, layout_tree: &LayoutTree) -> DisplayList {
    let mut dl = DisplayList::new();
    let mut inline_cursor_x: Option<f32> = None;
    let mut inline_cursor_y: Option<f32> = None;

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

        // Emit text commands for text nodes
        if let Some(dom_id) = node.dom_node_id {
            if let Some(dom_node) = document.node(dom_id) {
                match &dom_node.kind {
                    NodeKind::Text(text) => {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            let content_rect = node.content_rect();
                            let text_color = node
                                .style
                                .color
                                .map(to_gpu_color)
                                .unwrap_or(Color::BLACK);
                            let font_size = node.style.font_size.unwrap_or(16.0);
                            let bold = node.style.font_weight == FontWeight::Bold;
                            let italic = node.style.font_style == FontStyle::Italic;
                            let is_inline = node.style.display == Display::Inline;

                            let render_x = if is_inline {
                                if let (Some(cx), Some(cy)) = (inline_cursor_x, inline_cursor_y) {
                                    if (content_rect.y - cy).abs() < 1.0 {
                                        cx
                                    } else {
                                        inline_cursor_x = None;
                                        content_rect.x
                                    }
                                } else {
                                    content_rect.x
                                }
                            } else {
                                content_rect.x
                            };

                            dl.push_text(DrawText {
                                x: render_x,
                                y: content_rect.y,
                                text: trimmed.to_string(),
                                font_size,
                                color: text_color,
                                font_family: Some("sans-serif".to_string()),
                                bold,
                                italic,
                            });

                            if is_inline {
                                let text_width = trimmed.chars().count() as f32 * font_size * 0.6;
                                inline_cursor_x = Some(render_x + text_width);
                                inline_cursor_y = Some(content_rect.y);
                            }
                        }
                    }
                    NodeKind::Element(el) if el.tag_name.eq_ignore_ascii_case("img") => {
                        dl.push_rect(DrawRect {
                            x: node.rect.x,
                            y: node.rect.y,
                            width: node.rect.width,
                            height: node.rect.height,
                            color: Color::from_rgba8(200, 200, 200, 255),
                        });
                    }
                    _ => {}
                }
            }
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

    fn find_text(dl: &DisplayList) -> Vec<&DrawText> {
        let mut texts = Vec::new();
        for cmd in dl.commands() {
            if let kore_gpu::DisplayCommand::Text(t) = cmd {
                texts.push(t);
            }
        }
        texts
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
            "#empty { background-color: red; }",
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

    #[test]
    fn test_paragraph_text_emits_drawtext() {
        let (_, _, dl) = run_render(
            r#"<p id="p1">Hello World</p>"#,
            "#p1 { color: red; }",
        );
        let texts = find_text(&dl);
        assert!(!texts.is_empty(), "should have at least one text command");
        let has_hello = texts.iter().any(|t| t.text.contains("Hello World"));
        assert!(has_hello, "should contain 'Hello World' text");
        let has_red = texts.iter().any(|t| (t.color.r - 1.0).abs() < 0.01);
        assert!(has_red, "should have red colored text");
    }

    #[test]
    fn test_heading_has_bold_and_larger_font() {
        let (_, _, dl) = run_render(
            r#"<h1 id="h">Heading</h1>"#,
            "",
        );
        let texts = find_text(&dl);
        let heading = texts.iter().find(|t| t.text.contains("Heading"));
        assert!(heading.is_some(), "should have heading text");
        let h = heading.unwrap();
        assert!(h.bold, "h1 should be bold");
        assert!(h.font_size >= 24.0, "h1 should have large font size");
    }

    #[test]
    fn test_text_color_from_css() {
        let (_, _, dl) = run_render(
            r#"<p id="tc">colored text</p>"#,
            "#tc { color: #0000ff; }",
        );
        let texts = find_text(&dl);
        let colored = texts.iter().find(|t| t.text.contains("colored"));
        assert!(colored.is_some(), "should have colored text");
        let c = colored.unwrap();
        assert!((c.color.b - 1.0).abs() < 0.01, "text should be blue");
    }

    #[test]
    fn test_bold_and_italic_from_css() {
        let (_, _, dl) = run_render(
            r#"<p><b id="b">Bold</b><i id="i">Italic</i></p>"#,
            "",
        );
        let texts = find_text(&dl);
        let bold = texts.iter().find(|t| t.text.contains("Bold"));
        let italic = texts.iter().find(|t| t.text.contains("Italic"));
        assert!(bold.is_some(), "should have Bold text");
        assert!(italic.is_some(), "should have Italic text");
        if let Some(b) = bold {
            assert!(b.bold, "Bold tag should produce bold text");
        }
        if let Some(i) = italic {
            assert!(i.italic, "Italic tag should produce italic text");
        }
    }

    #[test]
    fn test_block_elements_stack_vertically() {
        let (_, _, dl) = run_render(
            r#"<div>First</div><div>Second</div>"#,
            "",
        );
        let texts = find_text(&dl);
        let first = texts.iter().find(|t| t.text.contains("First")).unwrap();
        let second = texts.iter().find(|t| t.text.contains("Second")).unwrap();
        assert!(second.y > first.y, "second block should be below first");
    }

    #[test]
    fn test_inline_elements_share_line() {
        let (_, _, dl) = run_render(
            r#"<span>Left</span><span>Right</span>"#,
            "",
        );
        let texts = find_text(&dl);
        let left = texts.iter().find(|t| t.text.contains("Left")).unwrap();
        let right = texts.iter().find(|t| t.text.contains("Right")).unwrap();
        assert!(
            (right.y - left.y).abs() < 1.0,
            "inline elements should be on the same line (y difference: {})",
            (right.y - left.y).abs()
        );
    }

    #[test]
    fn test_heading_margin_gives_vertical_space() {
        let (_, _, dl) = run_render(
            r#"<h1>Heading</h1><p>Paragraph</p>"#,
            "",
        );
        let texts = find_text(&dl);
        let heading = texts.iter().find(|t| t.text.contains("Heading")).unwrap();
        let para = texts.iter().find(|t| t.text.contains("Paragraph")).unwrap();
        // h1 default font-size is 32px, so line-height is ~44.8px
        // h1 margin-bottom is 32px, p margin-top is 16px
        // Gap from heading baseline to paragraph top should be > line-height
        let gap = para.y - heading.y;
        assert!(
            gap > heading.font_size,
            "paragraph should be below heading with margin (gap: {})",
            gap
        );
    }

    #[test]
    fn test_img_placeholder_rect() {
        let (_, _, dl) = run_render(
            r#"<img src="photo.jpg" width="200" height="150">"#,
            "",
        );
        let gray = Color::from_rgba8(200, 200, 200, 255);
        let has_gray = dl.commands().iter().any(|cmd| {
            if let kore_gpu::DisplayCommand::Rect(r) = cmd {
                (r.color.r - gray.r).abs() < 0.01
                    && (r.color.g - gray.g).abs() < 0.01
                    && (r.color.b - gray.b).abs() < 0.01
                    && (r.width - 200.0).abs() < 1.0
                    && (r.height - 150.0).abs() < 1.0
            } else {
                false
            }
        });
        assert!(has_gray, "img should have a gray 200x150 placeholder rect");
    }

    #[test]
    fn test_img_placeholder_default_size() {
        let (_, _, dl) = run_render(
            r#"<img src="photo.jpg">"#,
            "",
        );
        let gray = Color::from_rgba8(200, 200, 200, 255);
        let has_gray = dl.commands().iter().any(|cmd| {
            if let kore_gpu::DisplayCommand::Rect(r) = cmd {
                (r.color.r - gray.r).abs() < 0.01
                    && (r.color.g - gray.g).abs() < 0.01
                    && (r.color.b - gray.b).abs() < 0.01
                    && (r.width - 100.0).abs() < 1.0
                    && (r.height - 100.0).abs() < 1.0
            } else {
                false
            }
        });
        assert!(has_gray, "img should have a gray 100x100 placeholder rect");
    }

    #[test]
    fn test_line_height_scales_with_font_size() {
        let (_, _, dl) = run_render(
            r#"<p id="big">Text</p>"#,
            "#big { font-size: 20px; }",
        );
        let texts = find_text(&dl);
        let t = texts.iter().find(|t| t.text.contains("Text")).unwrap();
        assert!((t.font_size - 20.0).abs() < 0.01, "font size should be 20px");
    }
}
