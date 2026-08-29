use kore_css::{parse_stylesheet, CssColor};
use kore_gpu::{ClipRect, Color, DisplayList, DrawImage, DrawRect, DrawText, GpuImage};
use kore_html::{parse_document, NodeId, NodeKind};
use kore_layout::{layout_document, Display, FontStyle, FontWeight, LayoutConfig, LayoutTree};
use kore_net::{
    BlockReason, FetchRequest, Fetcher, HttpClient, Method, TrackingDecision, TrackingProtection,
};
use std::collections::HashMap;
use std::sync::Arc;
use url::Url;

use crate::error::PipelineError;
use crate::image::{decode_data_url, decode_image_bytes};

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
noscript { display: none !important; }
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
    pub js_navigation: Option<String>,
}

/// Outcome of running the pipeline on an already-fetched document.
enum RenderResult {
    Done(RenderOutput),
    Navigated(Url),
}

/// A rendered nested document (an `<iframe>`): its own display list plus the
/// position of the frame box in the parent layout.
#[derive(Debug, Clone)]
pub struct NestedFrame {
    pub display_list: DisplayList,
    pub links: Vec<(f32, f32, f32, f32, String)>,
    pub x: f32,
    pub y: f32,
}

/// The render pipeline: fetch HTML → parse → find CSS → fetch CSS → cascade → layout → display list.
///
/// All network access goes through a [`Fetcher`], which may run in-process
/// (default [`HttpClient`]) or in the dedicated network process.
pub struct Pipeline {
    fetcher: Arc<dyn Fetcher>,
    storage: kore_js::SharedStorage,
    cookies: kore_js::SharedCookieJar,
    tracking: TrackingProtection,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::with_http_client(HttpClient::default())
    }
}

impl Pipeline {
    /// Use an arbitrary fetcher (e.g. a remote one backed by the network
    /// process).
    pub fn new(fetcher: Arc<dyn Fetcher>) -> Self {
        Self {
            fetcher,
            storage: Arc::new(std::sync::Mutex::new(kore_js::WebStorage::default())),
            cookies: Arc::new(std::sync::Mutex::new(kore_js::CookieJar::default())),
            tracking: TrackingProtection::new(),
        }
    }

    /// Shared storage backing `localStorage` and `document.cookie`, so they
    /// survive navigation. Hand these out to DevTools to inspect live data.
    pub fn storage(&self) -> kore_js::SharedStorage {
        self.storage.clone()
    }

    pub fn cookies(&self) -> kore_js::SharedCookieJar {
        self.cookies.clone()
    }

    /// Shared tracking protection (ETP): check blocks, toggle, and read the
    /// block log from anywhere (e.g. DevTools).
    pub fn tracking(&self) -> TrackingProtection {
        self.tracking.clone()
    }

    /// Turn Enhanced Tracking Protection on/off.
    pub fn set_etp_enabled(&self, enabled: bool) {
        self.tracking.set_enabled(enabled);
    }

    /// Use an in-process HTTP client.
    pub fn with_http_client(http_client: HttpClient) -> Self {
        Self::new(Arc::new(http_client))
    }

    pub fn fetcher(&self) -> &Arc<dyn Fetcher> {
        &self.fetcher
    }

    /// Run the full pipeline: fetch, parse, style, layout, and build a display list.
    pub async fn render(&self, url: &Url) -> Result<RenderOutput, PipelineError> {
        let mut current_url = url.clone();
        for _hop in 0..5 {
            let html_str = self.fetch_html(&current_url).await?;
            match self.render_document(&html_str, &current_url).await? {
                RenderResult::Done(output) => return Ok(output),
                RenderResult::Navigated(next) => {
                    current_url = next;
                }
            }
        }
        Err(PipelineError::RedirectLimit)
    }

    /// Run the pipeline on an already-fetched HTML document: parse it, run
    /// scripts, apply styles, lay out, and build the display list (including
    /// nested `<iframe>` frames).
    async fn render_document(
        &self,
        html: &str,
        current_url: &Url,
    ) -> Result<RenderResult, PipelineError> {
        let document = Arc::new(std::sync::Mutex::new(parse_document(html)?));

        // `<meta http-equiv="refresh" content="0; url=…">` redirects must be
        // followed before anything else: search engines often serve a
        // placeholder document with an immediate meta-refresh instead of a
        // direct answer (e.g. Google's first hit on /search without cookies).
        if let Some((target, 0)) = document
            .lock()
            .ok()
            .and_then(|d| meta_refresh_target(&d, current_url))
        {
            return Ok(RenderResult::Navigated(
                target.unwrap_or_else(|| current_url.clone()),
            ));
        }

        let mut js_navigation: Option<String> = None;

        if let Ok(js_runtime) = kore_js::JsRuntime::with_shared_storage(
            document.clone(),
            self.storage.clone(),
            self.cookies.clone(),
        ) {
            let entries = collect_script_entries({
                let d = document.lock().unwrap();
                d.clone()
            });

            for entry in &entries {
                match entry {
                    ScriptEntry::Inline(content) => {
                        let _ = js_runtime.eval(content);
                        let _ = js_runtime.run_jobs();
                    }
                    ScriptEntry::External(url) => {
                        if let Ok(request) = FetchRequest::get(url.as_str()) {
                            if let Ok(response) = self.fetcher.fetch(request).await {
                                let body = String::from_utf8_lossy(&response.body).to_string();
                                let _ = js_runtime.eval(&body);
                                let _ = js_runtime.run_jobs();
                            }
                        }
                    }
                }
            }

            let _ = js_runtime.dispatch_dom_content_loaded();
            let _ = js_runtime.flush_timers();

            js_navigation = js_runtime
                .pending_navigation
                .lock()
                .ok()
                .and_then(|mut nav| nav.take());
        }

        if let Some(ref nav_url) = js_navigation {
            if let Ok(new_url) = url::Url::parse(nav_url) {
                return Ok(RenderResult::Navigated(new_url));
            }
        }

        let title = {
            let d = document.lock().unwrap();
            page_title(&d)
        };

        let mut stylesheets = vec![DEFAULT_CSS.to_string()];

        let css_futures: Vec<_> = {
            let d = document.lock().unwrap();
            linked_stylesheets(&d, current_url)
        }
            .into_iter()
            .map(|css_url| {
                let url = css_url.clone();
                async move { self.fetch_css(&url).await }
            })
            .collect();
        for result in futures::future::join_all(css_futures).await {
            if let Ok(css) = result {
                stylesheets.push(css);
            }
        }

        let combined_css = stylesheets.join("\n");
        let stylesheet = parse_stylesheet(&combined_css)?;

        let (width, height) = (1264.0, 628.0);
        let (display_list, links) = {
            let d = document.lock().unwrap();
            let layout_tree = layout_document(
                &d,
                &stylesheet,
                LayoutConfig {
                    viewport_width: width,
                    viewport_height: height,
                },
            )?;
            let iframes = self.render_iframes(&d, &layout_tree, current_url, 0).await;
            let images = self.fetch_images(&d, current_url).await;
            let dl = build_display_list_with_iframes(&d, &layout_tree, &images, current_url, &iframes);
            let mut links = extract_links(&d, &layout_tree);
            for frame in iframes.values() {
                for (lx, ly, lw, lh, href) in &frame.links {
                    links.push((frame.x + 1.0 + lx, frame.y + 1.0 + ly, *lw, *lh, href.clone()));
                }
            }
            (dl, links)
        };

        Ok(RenderResult::Done(RenderOutput {
            display_list,
            title,
            links,
            js_navigation,
        }))
    }

    /// Fetch and decode every `<img>` in `document` into a map keyed by
    /// resolved URL / raw `data:` URL.
    async fn fetch_images(
        &self,
        document: &kore_html::Document,
        base: &Url,
    ) -> HashMap<String, GpuImage> {
        let image_keys = image_sources(document, base);
        let image_futures: Vec<_> = image_keys
            .iter()
            .filter_map(|(key, url)| {
                let url = url.as_ref()?;
                if self.tracking.check(url, base.host_str()) == TrackingDecision::Block(BlockReason::TrackerDomain) {
                    return None;
                }
                let key = key.clone();
                let url = url.clone();
                Some(async move {
                    let request = FetchRequest::get(url.as_str()).ok()?;
                    let response = self.fetcher.fetch(request).await.ok()?;
                    let image = decode_image_bytes(&response.body)?;
                    Some((key, image))
                })
            })
            .collect();
        let mut images: HashMap<String, GpuImage> = HashMap::new();
        for result in futures::future::join_all(image_futures).await {
            if let Some((key, image)) = result {
                images.insert(key, image);
            }
        }
        for (key, url) in &image_keys {
            if url.is_none() {
                if let Some(image) = decode_data_url(key) {
                    images.insert(key.clone(), image);
                }
            }
        }
        images
    }

    /// Render the nested document of every `<iframe>` in the layout tree,
    /// laid out at the iframe's own size. Recurses into nested frames up to
    /// a depth limit.
    async fn render_iframes(
        &self,
        document: &kore_html::Document,
        layout_tree: &LayoutTree,
        base: &Url,
        depth: usize,
    ) -> HashMap<NodeId, NestedFrame> {
        if depth >= 3 {
            return HashMap::new();
        }

        struct Spec {
            dom_id: NodeId,
            key: String,
            content: Option<String>,
            fetch: Option<Url>,
            nested_base: Url,
            x: f32,
            y: f32,
            w: f32,
            h: f32,
        }

        let mut specs: Vec<Spec> = Vec::new();
        for node in &layout_tree.nodes {
            let Some(dom_id) = node.dom_node_id else { continue };
            let Some(dom_node) = document.node(dom_id) else { continue };
            let NodeKind::Element(el) = &dom_node.kind else { continue };
            if !el.tag_name.eq_ignore_ascii_case("iframe") {
                continue;
            }
            if node.rect.width <= 0.0 || node.rect.height <= 0.0 {
                continue;
            }
            let srcdoc = get_attribute(el, "srcdoc");
            let src = get_attribute(el, "src");
            let (key, content, fetch, nested_base) = match (srcdoc, src) {
                (Some(doc), _) => (format!("srcdoc:{doc}"), Some(doc), None, base.clone()),
                (None, Some(src)) if !src.starts_with("data:") => match base.join(&src) {
                    Ok(u) => {
                        if self.tracking.check(&u, base.host_str()) == TrackingDecision::Block(BlockReason::TrackerDomain) {
                            continue;
                        }
                        (u.as_str().to_string(), None, Some(u.clone()), u)
                    }
                    Err(_) => continue,
                },
                _ => continue,
            };
            specs.push(Spec {
                dom_id,
                key,
                content,
                fetch,
                nested_base,
                x: node.rect.x,
                y: node.rect.y,
                w: node.rect.width,
                h: node.rect.height,
            });
        }

        let mut bodies: HashMap<String, String> = HashMap::new();
        let unique_fetches: Vec<(String, Url)> = specs
            .iter()
            .filter_map(|s| s.fetch.clone().map(|u| (s.key.clone(), u)))
            .collect::<HashMap<_, _>>()
            .into_iter()
            .collect();
        let results = futures::future::join_all(unique_fetches.iter().map(|(key, url)| {
            let key = key.clone();
            let url = url.clone();
            async move {
                let request = FetchRequest::get(url.as_str()).ok()?;
                let response = self.fetcher.fetch(request).await.ok()?;
                Some((key, String::from_utf8_lossy(&response.body).to_string()))
            }
        }))
        .await;
        for result in results {
            if let Some((key, body)) = result {
                bodies.insert(key, body);
            }
        }

        let mut frames: HashMap<NodeId, NestedFrame> = HashMap::new();
        let mut cache: HashMap<String, NestedFrame> = HashMap::new();
        for spec in specs {
            if frames.contains_key(&spec.dom_id) {
                continue;
            }
            if let Some(frame) = cache.get(&spec.key) {
                frames.insert(
                    spec.dom_id,
                    NestedFrame {
                        display_list: frame.display_list.clone(),
                        links: frame.links.clone(),
                        x: spec.x,
                        y: spec.y,
                    },
                );
                continue;
            }
            let content = match spec.fetch {
                Some(_) => match bodies.get(&spec.key) {
                    Some(body) => body.clone(),
                    None => continue,
                },
                None => spec.content.clone().unwrap_or_default(),
            };
            let Ok(nested_doc) = parse_document(&content) else { continue };
            let nested_doc = Arc::new(std::sync::Mutex::new(nested_doc));
            let Ok(stylesheet) = parse_stylesheet(DEFAULT_CSS) else { continue };
            let Ok(nested_tree) = layout_document(
                &nested_doc.lock().unwrap(),
                &stylesheet,
                LayoutConfig {
                    viewport_width: spec.w,
                    viewport_height: spec.h,
                },
            ) else {
                continue;
            };
            let nested_images = self
                .fetch_images(&nested_doc.lock().unwrap(), &spec.nested_base)
                .await;
            let nested_iframes = Box::pin(self.render_iframes(
                &nested_doc.lock().unwrap(),
                &nested_tree,
                &spec.nested_base,
                depth + 1,
            ))
            .await;
            let dl = build_display_list_with_iframes(
                &nested_doc.lock().unwrap(),
                &nested_tree,
                &nested_images,
                &spec.nested_base,
                &nested_iframes,
            );
            let links = extract_links(&nested_doc.lock().unwrap(), &nested_tree);
            let frame = NestedFrame {
                display_list: dl,
                links,
                x: spec.x,
                y: spec.y,
            };
            cache.insert(spec.key, frame.clone());
            frames.insert(spec.dom_id, frame);
        }
        frames
    }

    /// Submit an HTML form: collect its controls, then either navigate to
    /// `action?<urlencoded>` (GET) or POST the urlencoded body to `action`.
    pub async fn submit_form(
        &self,
        document: &kore_html::Document,
        form_id: NodeId,
        base: &Url,
    ) -> Result<RenderOutput, PipelineError> {
        let fields = collect_form_data(document, form_id);
        let action = form_action(document, form_id).unwrap_or_else(|| base.to_string());
        let method = form_method(document, form_id);
        let mut action_url = base.join(&action)?;
        let encoded = urlencode(&fields);

        if method.eq_ignore_ascii_case("post") {
            let request = FetchRequest {
                url: action_url,
                method: Method::Post,
                body: Some(bytes::Bytes::from(encoded.into_bytes())),
                headers: vec![(
                    "content-type".to_string(),
                    "application/x-www-form-urlencoded".to_string(),
                )],
                top_level: None,
            };
            let response = self
                .fetcher
                .fetch(request)
                .await
                .map_err(PipelineError::Network)?;
            let html = String::from_utf8(response.body.to_vec())
                .map_err(|_| PipelineError::InvalidUtf8)?;
            match self.render_document(&html, &response.final_url).await? {
                RenderResult::Done(output) => Ok(output),
                RenderResult::Navigated(next) => self.render(&next).await,
            }
        } else {
            let existing = action_url.query();
            let query = match existing {
                Some(q) if !q.is_empty() => format!("{q}&{encoded}"),
                _ => encoded,
            };
            action_url.set_query(Some(&query));
            self.render(&action_url).await
        }
    }

    async fn fetch_html(&self, url: &Url) -> Result<String, PipelineError> {
        if is_about_blank(url) {
            return Ok(String::new());
        }
        let request = FetchRequest::get(url.as_str())?;
        let response = self
            .fetcher
            .fetch(request)
            .await
            .map_err(PipelineError::Network)?;
        String::from_utf8(response.body.to_vec()).map_err(|_| PipelineError::InvalidUtf8)
    }

    async fn fetch_css(&self, url: &Url) -> Result<String, PipelineError> {
        let request = FetchRequest::get(url.as_str())?;
        let response = self
            .fetcher
            .fetch(request)
            .await
            .map_err(PipelineError::Network)?;
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

/// Parse `<meta http-equiv="refresh">`, if any, returning the optional target
/// URL (`None` = reload the current page) and the delay in seconds.
fn meta_refresh_target(document: &kore_html::Document, base: &Url) -> Option<(Option<Url>, u64)> {
    for node in document.nodes() {
        if let NodeKind::Element(el) = &node.kind {
            if !el.tag_name.eq_ignore_ascii_case("meta") {
                continue;
            }
            let is_refresh = el.attributes.iter().any(|attr| {
                attr.name.eq_ignore_ascii_case("http-equiv")
                    && attr.value.eq_ignore_ascii_case("refresh")
            });
            if !is_refresh {
                continue;
            }
            let content = el
                .attributes
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case("content"))
                .map(|a| a.value.as_str())
                .unwrap_or("");
            let (delay_str, url_part) = match content.split_once(';') {
                Some((delay, rest)) => (delay.trim(), rest.trim()),
                None => (content.trim(), ""),
            };
            let delay = delay_str.parse::<u64>().unwrap_or(0);
            let target = if url_part.is_empty() {
                None
            } else {
                let rest = match url_part.get(..4).map(|p| p.eq_ignore_ascii_case("url=")) {
                    Some(true) => &url_part[4..],
                    _ => url_part,
                };
                let cleaned = rest.trim_matches(['\'', '"']);
                base.join(cleaned).ok()
            };
            return Some((target, delay));
        }
    }
    None
}

/// True when `node_id` is a descendant of a `<noscript>` element. JavaScript
/// is always available in Kore (there is no "disable JS" setting), so
/// `<noscript>` content is hidden and never processed: no stylesheets, no
/// scripts, no images, no links — matching a browser with JS enabled.
fn inside_noscript(document: &kore_html::Document, mut node_id: NodeId) -> bool {
    let mut depth = 0;
    while let Some(node) = document.node(node_id) {
        depth += 1;
        if depth > 4096 {
            return false;
        }
        if let NodeKind::Element(el) = &node.kind {
            if el.tag_name.eq_ignore_ascii_case("noscript") {
                return true;
            }
        }
        match node.parent {
            Some(parent) => node_id = parent,
            None => return false,
        }
    }
    false
}

/// Find `<link rel="stylesheet">` elements and resolve their href to absolute URLs.
pub fn linked_stylesheets(document: &kore_html::Document, base: &Url) -> Vec<Url> {
    let mut urls = Vec::new();
    for node in document.nodes() {
        if inside_noscript(document, node.id) {
            continue;
        }
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

/// A script found in the document, either inline or external.
enum ScriptEntry {
    Inline(String),
    External(String),
}

/// Collect all `<script>` entries (inline content or external URL) from the document.
fn collect_script_entries(document: kore_html::Document) -> Vec<ScriptEntry> {
    let mut entries = Vec::new();
    collect_scripts_recursive(&document, document.root(), &mut entries);
    entries
}

fn collect_scripts_recursive(
    document: &kore_html::Document,
    node_id: kore_html::NodeId,
    entries: &mut Vec<ScriptEntry>,
) {
    let node = match document.node(node_id) {
        Some(n) => n,
        None => return,
    };

    if inside_noscript(document, node_id) {
        return;
    }

    if let NodeKind::Element(el) = &node.kind {
        if el.tag_name.eq_ignore_ascii_case("script") {
            let has_src = el.attributes.iter().any(|a| a.name.eq_ignore_ascii_case("src"));
            let script_type = el.attributes.iter()
                .find(|a| a.name.eq_ignore_ascii_case("type"))
                .map(|a| a.value.as_str())
                .unwrap_or("text/javascript");

            let is_js = script_type == "text/javascript"
                || script_type == "application/javascript"
                || script_type == "";

            if is_js {
                if !has_src {
                    let content = get_script_text(document, node_id);
                    if !content.trim().is_empty() {
                        entries.push(ScriptEntry::Inline(content));
                    }
                } else if let Some(src) = el.attributes.iter()
                    .find(|a| a.name.eq_ignore_ascii_case("src"))
                    .map(|a| a.value.clone())
                {
                    if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("//") {
                        let full_url = if src.starts_with("//") {
                            format!("https:{}", src)
                        } else {
                            src
                        };
                        entries.push(ScriptEntry::External(full_url));
                    }
                }
            }
        }
    }

    for &child_id in &node.children.clone() {
        collect_scripts_recursive(document, child_id, entries);
    }
}

fn get_script_text(document: &kore_html::Document, node_id: kore_html::NodeId) -> String {
    let node = match document.node(node_id) {
        Some(n) => n,
        None => return String::new(),
    };
    let mut text = String::new();
    for &child_id in &node.children {
        if let Some(child) = document.node(child_id) {
            if let NodeKind::Text(t) = &child.kind {
                text.push_str(t);
            }
        }
    }
    text
}

/// Convert a CssColor (kore-css) to a Color (kore-gpu).
fn parse_inline_style(attributes: &[kore_html::Attribute]) -> (Option<Color>, Option<f32>, bool) {
    let style_str = attributes
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("style"))
        .map(|a| a.value.as_str())
        .unwrap_or("");

    if style_str.is_empty() {
        return (None, None, false);
    }

    let mut color = None;
    let mut font_size = None;
    let mut bold = false;

    for decl in style_str.split(';') {
        let parts: Vec<&str> = decl.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let prop = parts[0].trim().to_lowercase();
        let val = parts[1].trim();

        match prop.as_str() {
            "color" => {
                if let Some(css_color) = kore_css::parse_color(val) {
                    color = Some(to_gpu_color(css_color));
                }
            }
            "font-size" => {
                if let Some(px) = val.strip_suffix("px") {
                    font_size = px.trim().parse().ok();
                } else if let Some(em) = val.strip_suffix("em") {
                    if let Ok(v) = em.trim().parse::<f32>() {
                        font_size = Some(v * 16.0);
                    }
                }
            }
            "font-weight" => {
                bold = val == "bold" || val == "700" || val == "800" || val == "900";
            }
            _ => {}
        }
    }

    (color, font_size, bold)
}

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
            if default_display_for_tag(&el.tag_name) == Display::None {
                return;
            }

            if let Some(ln) = layout_tree.nodes.iter().find(|n| n.dom_node_id == Some(dom_id)) {
                if ln.rect.width > 0.0 && ln.rect.height > 0.0 {
                    if let Some(color) = ln.style.background_color.map(to_gpu_color).or_else(|| default_bg_color(&el.tag_name)) {
                        dl.push_rect(DrawRect { x: ln.rect.x, y: ln.rect.y, width: ln.rect.width, height: ln.rect.height, color, opacity: 1.0, translate: (0.0, 0.0) });
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
                    opacity: 1.0,
                    translate: (0.0, 0.0),
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

fn get_attribute(el: &kore_html::Element, name: &str) -> Option<String> {
    el.attributes
        .iter()
        .find(|attr| attr.name.eq_ignore_ascii_case(name))
        .map(|attr| attr.value.clone())
}

/// The map key for an image: the resolved absolute URL, or the raw
/// `data:` URL (decoded locally without a network fetch).
fn image_key(src: &str, base: &Url) -> String {
    if src.starts_with("data:") {
        src.to_string()
    } else {
        match base.join(src) {
            Ok(url) => url.as_str().to_string(),
            Err(_) => src.to_string(),
        }
    }
}

/// Collect every `<img src>` in the document. `None` marks data URLs,
/// which are decoded locally rather than fetched.
fn image_sources(document: &kore_html::Document, base: &Url) -> Vec<(String, Option<Url>)> {
    let mut sources = Vec::new();
    for node in document.nodes() {
        if inside_noscript(document, node.id) {
            continue;
        }
        if let NodeKind::Element(el) = &node.kind {
            if el.tag_name.eq_ignore_ascii_case("img") {
                if let Some(src) = get_attribute(el, "src") {
                    if src.starts_with("data:") {
                        sources.push((src, None));
                    } else if let Ok(url) = base.join(&src) {
                        sources.push((url.as_str().to_string(), Some(url)));
                    }
                }
            }
        }
    }
    sources
}

/// The `action` attribute of a form, if any.
pub fn form_action(document: &kore_html::Document, form_id: NodeId) -> Option<String> {
    let node = document.node(form_id)?;
    let NodeKind::Element(el) = &node.kind else {
        return None;
    };
    if !el.tag_name.eq_ignore_ascii_case("form") {
        return None;
    }
    get_attribute(el, "action")
}

/// The submission method of a form: `"get"` by default, `"post"` when the
/// `method` attribute says so.
pub fn form_method(document: &kore_html::Document, form_id: NodeId) -> String {
    let node = document.node(form_id);
    let method = node.and_then(|n| match &n.kind {
        NodeKind::Element(el) => get_attribute(el, "method"),
        _ => None,
    });
    match method.as_deref() {
        Some(m) if m.eq_ignore_ascii_case("post") => "post".to_string(),
        _ => "get".to_string(),
    }
}

/// Collect the `name=value` pairs of every control in a `<form>`, using the
/// controls' static attribute values:
/// - `input` — `value` attribute (checked `checkbox`/`radio` use their
///   value or `"on"`; unchecked ones are skipped)
/// - `select` — the selected option's value or text
/// - `textarea` — its text content
/// - `button` — its `value` attribute
pub fn collect_form_data(document: &kore_html::Document, form_id: NodeId) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut stack = vec![form_id];
    while let Some(id) = stack.pop() {
        let Some(node) = document.node(id) else {
            continue;
        };
        stack.extend(node.children.iter().rev().copied());
        let NodeKind::Element(el) = &node.kind else {
            continue;
        };
        let Some(name) = get_attribute(el, "name") else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        match el.tag_name.to_ascii_lowercase().as_str() {
            "input" => {
                let input_type = get_attribute(el, "type")
                    .map(|t| t.to_ascii_lowercase())
                    .unwrap_or_else(|| "text".to_string());
                match input_type.as_str() {
                    "checkbox" | "radio" => {
                        if get_attribute(el, "checked").is_some() {
                            pairs.push((
                                name,
                                get_attribute(el, "value").unwrap_or_else(|| "on".to_string()),
                            ));
                        }
                    }
                    _ => pairs.push((name, get_attribute(el, "value").unwrap_or_default())),
                }
            }
            "select" => pairs.push((name, selected_option(document, id))),
            "textarea" => {
                let text: String = node
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
                    .collect();
                pairs.push((name, text.trim().to_string()));
            }
            "button" => {
                if let Some(value) = get_attribute(el, "value") {
                    pairs.push((name, value));
                }
            }
            _ => {}
        }
    }
    pairs
}

/// The selected `<option>` of a `<select>`: the one with the `selected`
/// attribute, or the first option.
fn selected_option(document: &kore_html::Document, select_id: NodeId) -> String {
    let mut first: Option<String> = None;
    let Some(select) = document.node(select_id) else {
        return String::new();
    };
    for child_id in &select.children {
        let Some(child) = document.node(*child_id) else {
            continue;
        };
        let NodeKind::Element(el) = &child.kind else {
            continue;
        };
        if !el.tag_name.eq_ignore_ascii_case("option") {
            continue;
        }
        let text: String = child
            .children
            .iter()
            .filter_map(|c| document.node(*c))
            .filter_map(|c| {
                if let NodeKind::Text(t) = &c.kind {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        let value = get_attribute(el, "value").unwrap_or_else(|| text.trim().to_string());
        if first.is_none() {
            first = Some(value.clone());
        }
        if get_attribute(el, "selected").is_some() {
            return value;
        }
    }
    first.unwrap_or_default()
}

/// Percent-encode a `name=value` pair list as an `application/x-www-form-urlencoded`
/// body or query string (`+` for spaces, `%XX` for everything else).
pub fn urlencode(pairs: &[(String, String)]) -> String {
    let mut out = String::new();
    for (i, (key, value)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        out.push_str(&percent_encode(key));
        out.push('=');
        out.push_str(&percent_encode(value));
    }
    out
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build a DisplayList from a LayoutTree and its associated DOM.
pub fn build_display_list(document: &kore_html::Document, layout_tree: &LayoutTree) -> DisplayList {
    let base = match Url::parse("about:blank") {
        Ok(base) => base,
        Err(_) => return DisplayList::new(),
    };
    build_display_list_with_images(document, layout_tree, &HashMap::new(), &base)
}

/// Like [`build_display_list`], but resolves `<img>` elements against
/// `images` (a map from resolved URL / data-URL string to decoded pixels).
pub fn build_display_list_with_images(
    document: &kore_html::Document,
    layout_tree: &LayoutTree,
    images: &HashMap<String, GpuImage>,
    base_url: &Url,
) -> DisplayList {
    build_display_list_with_iframes(document, layout_tree, images, base_url, &HashMap::new())
}

/// Like [`build_display_list_with_images`], but embeds the pre-rendered
/// `<iframe>` contents from `iframes` (keyed by DOM node id).
pub fn build_display_list_with_iframes(
    document: &kore_html::Document,
    layout_tree: &LayoutTree,
    images: &HashMap<String, GpuImage>,
    base_url: &Url,
    iframes: &HashMap<NodeId, NestedFrame>,
) -> DisplayList {
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
                opacity: 1.0,
                translate: (0.0, 0.0),
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
                            let mut font_size = node.style.font_size.unwrap_or(16.0);
                            let mut bold = node.style.font_weight == FontWeight::Bold;
                            let italic = node.style.font_style == FontStyle::Italic;
                            let is_inline = node.style.display == Display::Inline;

                            // Check parent element for inline styles
                            let (inline_color, inline_font_size, inline_bold) =
                                if let Some(parent_id) = dom_node.parent {
                                    if let Some(parent) = document.node(parent_id) {
                                        if let NodeKind::Element(el) = &parent.kind {
                                            parse_inline_style(&el.attributes)
                                        } else {
                                            (None, None, false)
                                        }
                                    } else {
                                        (None, None, false)
                                    }
                                } else {
                                    (None, None, false)
                                };
                            let final_color = inline_color.unwrap_or(text_color);
                            if let Some(inline_fs) = inline_font_size {
                                font_size = inline_fs;
                            }
                            if inline_bold {
                                bold = true;
                            }
                            let baseline_offset = font_size * 0.8;

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
                                y: content_rect.y + baseline_offset,
                                text: trimmed.to_string(),
                                font_size,
                                color: final_color,
                                font_family: Some("sans-serif".to_string()),
                                bold,
                                italic,
                                opacity: 1.0,
                                translate: (0.0, 0.0),
                            });

                            if is_inline {
                                let text_width = trimmed.chars().count() as f32 * font_size * 0.6;
                                inline_cursor_x = Some(render_x + text_width);
                                inline_cursor_y = Some(content_rect.y);
                            }
                        }
                    }
                    NodeKind::Element(el) if el.tag_name.eq_ignore_ascii_case("img") => {
                        let key = get_attribute(el, "src").map(|src| image_key(&src, base_url));
                        let image = key.as_ref().and_then(|key| images.get(key));
                        if let Some(image) = image {
                            dl.push_image(DrawImage {
                                x: node.rect.x,
                                y: node.rect.y,
                                width: node.rect.width,
                                height: node.rect.height,
                                atlas_id: 0,
                                image: image.clone(),
                            });
                        } else {
                            dl.push_rect(DrawRect {
                                x: node.rect.x,
                                y: node.rect.y,
                                width: node.rect.width,
                                height: node.rect.height,
                                color: Color::from_rgba8(200, 200, 200, 255),
                                opacity: 1.0,
                                translate: (0.0, 0.0),
                            });
                        }
                    }
                    NodeKind::Element(el)
                        if el.tag_name.eq_ignore_ascii_case("video")
                            || el.tag_name.eq_ignore_ascii_case("audio") =>
                    {
                        let is_video = el.tag_name.eq_ignore_ascii_case("video");
                        let bg = if is_video {
                            Color::from_rgba8(16, 16, 16, 255)
                        } else {
                            Color::from_rgba8(226, 226, 226, 255)
                        };
                        dl.push_rect(DrawRect {
                            x: node.rect.x,
                            y: node.rect.y,
                            width: node.rect.width,
                            height: node.rect.height,
                            color: bg,
                            opacity: 1.0,
                            translate: (0.0, 0.0),
                        });
                        let label = if is_video {
                            format!("▶ video ({}×{})", node.rect.width as i32, node.rect.height as i32)
                        } else {
                            format!("♪ audio ({}×{})", node.rect.width as i32, node.rect.height as i32)
                        };
                        let text_color = if is_video {
                            Color::from_rgba8(235, 235, 235, 255)
                        } else {
                            Color::from_rgba8(70, 70, 70, 255)
                        };
                        dl.push_text(DrawText {
                            x: node.rect.x + 8.0,
                            y: node.rect.y + node.rect.height / 2.0 + 4.0,
                            text: label,
                            font_size: 14.0,
                            color: text_color,
                            font_family: Some("sans-serif".to_string()),
                            bold: false,
                            italic: false,
                            opacity: 1.0,
                            translate: (0.0, 0.0),
                        });
                    }
                    NodeKind::Element(el) if el.tag_name.eq_ignore_ascii_case("iframe") => {
                        match iframes.get(&dom_id) {
                            Some(frame) => {
                                dl.push_rect(DrawRect {
                                    x: node.rect.x,
                                    y: node.rect.y,
                                    width: node.rect.width,
                                    height: node.rect.height,
                                    color: Color::from_rgba8(130, 130, 130, 255),
                                    opacity: 1.0,
                                    translate: (0.0, 0.0),
                                });
                                dl.push_clip(ClipRect {
                                    x: node.rect.x + 1.0,
                                    y: node.rect.y + 1.0,
                                    width: node.rect.width - 2.0,
                                    height: node.rect.height - 2.0,
                                });
                                dl.merge_translated(
                                    &frame.display_list,
                                    node.rect.x + 1.0,
                                    node.rect.y + 1.0,
                                );
                                dl.pop_clip();
                            }
                            None => {
                                dl.push_rect(DrawRect {
                                    x: node.rect.x,
                                    y: node.rect.y,
                                    width: node.rect.width,
                                    height: node.rect.height,
                                    color: Color::from_rgba8(216, 216, 216, 255),
                                    opacity: 1.0,
                                    translate: (0.0, 0.0),
                                });
                                dl.push_text(DrawText {
                                    x: node.rect.x + 8.0,
                                    y: node.rect.y + node.rect.height / 2.0 + 4.0,
                                    text: "iframe".to_string(),
                                    font_size: 13.0,
                                    color: Color::from_rgba8(90, 90, 90, 255),
                                    font_family: Some("sans-serif".to_string()),
                                    bold: false,
                                    italic: false,
                                    opacity: 1.0,
                                    translate: (0.0, 0.0),
                                });
                            }
                        }
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

    fn base64_encode(data: &[u8]) -> String {
        const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
            let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
            let triple = (b0 << 16) | (b1 << 8) | b2;
            out.push(TABLE[(triple >> 18) as usize & 63] as char);
            out.push(TABLE[(triple >> 12) as usize & 63] as char);
            out.push(if chunk.len() > 1 { TABLE[(triple >> 6) as usize & 63] as char } else { '=' });
            out.push(if chunk.len() > 2 { TABLE[triple as usize & 63] as char } else { '=' });
        }
        out
    }

    #[test]
    fn meta_refresh_zero_delay_target_resolved() {
        let doc = parse_document(
            r#"<html><head><meta http-equiv="refresh" content="0; url=https://example.com/final"></head><body>x</body></html>"#,
        )
        .unwrap();
        let base = Url::parse("https://www.google.com/search").unwrap();
        let (target, delay) = meta_refresh_target(&doc, &base).unwrap();
        assert_eq!(delay, 0);
        assert_eq!(target.unwrap().as_str(), "https://example.com/final");
    }

    #[test]
    fn meta_refresh_relative_url_resolved_against_base() {
        let doc = parse_document(r#"<meta http-equiv=refresh content="0; url=/search?q=kore">"#).unwrap();
        let base = Url::parse("https://www.google.com/").unwrap();
        let (target, _) = meta_refresh_target(&doc, &base).unwrap();
        assert_eq!(
            target.unwrap().as_str(),
            "https://www.google.com/search?q=kore"
        );
    }

    #[test]
    fn meta_refresh_reload_without_url() {
        let doc = parse_document(r#"<meta http-equiv="refresh" content="0">"#).unwrap();
        let base = Url::parse("https://example.com/x").unwrap();
        let (target, delay) = meta_refresh_target(&doc, &base).unwrap();
        assert_eq!(delay, 0);
        assert!(target.is_none());
    }

    #[test]
    fn meta_refresh_quoted_url_and_delay_parsed() {
        let doc = parse_document(
            r#"<meta http-equiv="refresh" content="5; URL='https://example.com/after'">"#,
        )
        .unwrap();
        let base = Url::parse("https://example.com/").unwrap();
        let (target, delay) = meta_refresh_target(&doc, &base).unwrap();
        assert_eq!(delay, 5);
        assert_eq!(target.unwrap().as_str(), "https://example.com/after");
    }

    #[test]
    fn meta_refresh_absent_returns_none() {
        let doc = parse_document(r#"<meta name="viewport" content="width=device-width">"#).unwrap();
        let base = Url::parse("https://example.com/").unwrap();
        assert!(meta_refresh_target(&doc, &base).is_none());
    }

    #[test]
    fn noscript_images_not_collected() {
        let doc = parse_document(
            r#"<html><body><noscript><img src="https://example.com/noscript.png"></noscript><img src="https://example.com/normal.png"></body></html>"#,
        )
        .unwrap();
        let base = Url::parse("https://example.com/").unwrap();
        let sources = image_sources(&doc, &base);
        assert_eq!(sources.len(), 1);
        assert!(sources[0].1.as_ref().unwrap().as_str().ends_with("normal.png"));
    }

    #[test]
    fn noscript_stylesheets_not_collected() {
        let doc = parse_document(
            r#"<html><head><noscript><link rel="stylesheet" href="https://example.com/noscript.css"></noscript><link rel="stylesheet" href="https://example.com/normal.css"></head><body></body></html>"#,
        )
        .unwrap();
        let base = Url::parse("https://example.com/").unwrap();
        let css = linked_stylesheets(&doc, &base);
        assert_eq!(css.len(), 1);
        assert!(css[0].as_str().ends_with("normal.css"));
    }

    #[test]
    fn noscript_scripts_not_collected() {
        let doc = parse_document(
            r#"<html><head><noscript><script>window.x = 1;</script></noscript><script>window.y = 2;</script></head><body></body></html>"#,
        )
        .unwrap();
        let entries = collect_script_entries(doc);
        assert_eq!(entries.len(), 1);
        match &entries[0] {
            ScriptEntry::Inline(content) => assert!(content.contains("window.y")),
            ScriptEntry::External(_) => panic!("expected inline script"),
        }
    }

    #[test]
    fn noscript_subtree_absent_from_layout() {
        let doc = parse_document(
            r#"<html><body><noscript><p>Turn on JavaScript to keep searching</p></noscript><p>visible</p></body></html>"#,
        )
        .unwrap();
        let stylesheet = parse_stylesheet(DEFAULT_CSS).unwrap();
        let layout_tree = layout_document(
            &doc,
            &stylesheet,
            LayoutConfig {
                viewport_width: 800.0,
                viewport_height: 600.0,
            },
        )
        .unwrap();
        let text_present = |needle: &str| {
            layout_tree.nodes.iter().any(|node| {
                node.dom_node_id.and_then(|id| doc.node(id)).is_some_and(|n| {
                    matches!(&n.kind, NodeKind::Text(t) if t.contains(needle))
                })
            })
        };
        assert!(!text_present("Turn on JavaScript"));
        assert!(text_present("visible"));
    }

    #[test]
    fn data_url_img_emits_draw_image() {
        let src = format!(
            "data:image/png;base64,{}",
            base64_encode(&crate::image::test_png_bytes())
        );
        let html = format!(r#"<img src="{}" width="64" height="64">"#, src);
        let doc = parse_document(&html).unwrap();
        let combined = format!("{}\nimg {{ display: block; }}", DEFAULT_CSS);
        let stylesheet = parse_stylesheet(&combined).unwrap();
        let layout_tree = layout_document(
            &doc,
            &stylesheet,
            LayoutConfig {
                viewport_width: 800.0,
                viewport_height: 600.0,
            },
        )
        .unwrap();
        let mut images = HashMap::new();
        images.insert(src.clone(), decode_data_url(&src).unwrap());
        let base = Url::parse("https://example.com/page.html").unwrap();
        let dl = build_display_list_with_images(&doc, &layout_tree, &images, &base);
        let image_commands = dl
            .commands()
            .iter()
            .filter_map(|cmd| match cmd {
                kore_gpu::DisplayCommand::Image(img) => Some(img),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(image_commands.len(), 1);
        assert_eq!(image_commands[0].image.width, 2);
        assert_eq!(image_commands[0].image.height, 2);
        assert_eq!(image_commands[0].width, 64.0);
        assert_eq!(image_commands[0].height, 64.0);
    }

    #[test]
    fn unresolved_img_falls_back_to_placeholder() {
        let doc = parse_document(r#"<img src="missing.png" width="50" height="50">"#).unwrap();
        let combined = format!("{}\nimg {{ display: block; }}", DEFAULT_CSS);
        let stylesheet = parse_stylesheet(&combined).unwrap();
        let layout_tree = layout_document(
            &doc,
            &stylesheet,
            LayoutConfig {
                viewport_width: 800.0,
                viewport_height: 600.0,
            },
        )
        .unwrap();
        let dl = build_display_list_with_images(&doc, &layout_tree, &HashMap::new(), &Url::parse("https://example.com/").unwrap());
        let has_image = dl.commands().iter().any(|cmd| {
            matches!(cmd, kore_gpu::DisplayCommand::Image(_))
        });
        assert!(!has_image);
        let placeholder = dl.commands().iter().any(|cmd| {
            if let kore_gpu::DisplayCommand::Rect(rect) = cmd {
                rect.width == 50.0 && rect.height == 50.0
            } else {
                false
            }
        });
        assert!(placeholder);
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
    fn test_script_tag_executes_and_modifies_dom() {
        let doc = parse_document(r#"
            <html><body>
                <div id="target">original</div>
                <script>
                    var el = document.getElementById('target');
                    if (el) el.setAttribute('data-modified', 'true');
                </script>
            </body></html>
        "#).unwrap();
        let entries = collect_script_entries(doc.clone());
        assert!(!entries.is_empty(), "Should find script tag");
        match &entries[0] {
            ScriptEntry::Inline(content) => {
                assert!(content.contains("getElementById"));
            }
            _ => panic!("expected inline script"),
        }
    }

    #[test]
    fn test_collect_scripts_finds_inline_scripts() {
        let doc = parse_document(r#"<html><head>
            <script>var x = 1;</script>
            <script type="text/javascript">var y = 2;</script>
            <script src="https://example.com/lib.js"></script>
        </head></html>"#).unwrap();
        let entries = collect_script_entries(doc);
        assert_eq!(entries.len(), 3, "should find 2 inline + 1 external script");
        match &entries[0] {
            ScriptEntry::Inline(content) => assert!(content.contains("var x = 1")),
            _ => panic!("expected inline script"),
        }
        match &entries[1] {
            ScriptEntry::Inline(content) => assert!(content.contains("var y = 2")),
            _ => panic!("expected inline script"),
        }
        match &entries[2] {
            ScriptEntry::External(url) => assert_eq!(url, "https://example.com/lib.js"),
            _ => panic!("expected external script"),
        }
    }

    #[test]
    fn test_script_type_filtering() {
        let doc = parse_document(r#"<html><body>
            <script type="text/javascript">var a = 1;</script>
            <script type="text/css">.foo { color: red; }</script>
            <script type="application/json">{"key": "value"}</script>
        </body></html>"#).unwrap();
        let entries = collect_script_entries(doc);
        assert_eq!(entries.len(), 1, "should only execute text/javascript");
        match &entries[0] {
            ScriptEntry::Inline(content) => assert!(content.contains("var a = 1")),
            _ => panic!("expected inline script"),
        }
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

    #[test]
    fn video_audio_emit_placeholder_rect_and_label() {
        let (_, _, dl) = run_render(
            r#"<video src="movie.mp4"></video><audio src="song.mp3"></audio>"#,
            "",
        );
        let has_video_bg = dl.commands().iter().any(|cmd| {
            if let kore_gpu::DisplayCommand::Rect(r) = cmd {
                let c = Color::from_rgba8(16, 16, 16, 255);
                (r.color.r - c.r).abs() < 0.01
                    && (r.color.g - c.g).abs() < 0.01
                    && (r.color.b - c.b).abs() < 0.01
                    && (r.width - 300.0).abs() < 1.0
                    && (r.height - 150.0).abs() < 1.0
            } else {
                false
            }
        });
        let has_audio_bg = dl.commands().iter().any(|cmd| {
            if let kore_gpu::DisplayCommand::Rect(r) = cmd {
                let c = Color::from_rgba8(226, 226, 226, 255);
                (r.color.r - c.r).abs() < 0.01
                    && (r.color.g - c.g).abs() < 0.01
                    && (r.color.b - c.b).abs() < 0.01
                    && (r.width - 300.0).abs() < 1.0
                    && (r.height - 54.0).abs() < 1.0
            } else {
                false
            }
        });
        assert!(has_video_bg, "video should have a dark 300x150 placeholder rect");
        assert!(has_audio_bg, "audio should have a light 300x54 placeholder rect");

        let texts = find_text(&dl);
        assert!(
            texts.iter().any(|t| t.text.contains("▶")),
            "video should draw a play label"
        );
        assert!(
            texts.iter().any(|t| t.text.contains("♪")),
            "audio should draw a music label"
        );
    }

    #[test]
    fn iframe_without_frame_shows_placeholder() {
        let (doc, tree) = {
            let document = parse_document(r#"<iframe width="320" height="200"></iframe>"#).unwrap();
            let stylesheet = parse_stylesheet(DEFAULT_CSS).unwrap();
            let layout_tree = layout_document(
                &document,
                &stylesheet,
                LayoutConfig {
                    viewport_width: 800.0,
                    viewport_height: 600.0,
                },
            )
            .unwrap();
            (document, layout_tree)
        };
        let dl = build_display_list_with_iframes(
            &doc,
            &tree,
            &HashMap::new(),
            &Url::parse("https://example.com/").unwrap(),
            &HashMap::new(),
        );
        let texts = find_text(&dl);
        assert!(
            texts.iter().any(|t| t.text.contains("iframe")),
            "placeholder should label the iframe"
        );
    }

    fn find_form_id(document: &kore_html::Document) -> Option<NodeId> {
        document.nodes().iter().find(|n| {
            if let NodeKind::Element(el) = &n.kind {
                el.tag_name.eq_ignore_ascii_case("form")
            } else {
                false
            }
        })
        .map(|n| n.id)
    }

    #[test]
    fn collect_form_data_collects_controls() {
        let doc = parse_document(
            r#"<form>
                <input name="q" value="rust">
                <input name="hidden" type="hidden" value="x">
                <input name="remember" type="checkbox" checked>
                <input name="no" type="checkbox">
                <input name="pick" type="radio" value="b" checked>
                <select name="fruit">
                    <option>apple</option>
                    <option value="banana" selected>banana</option>
                </select>
                <textarea name="note">hello world</textarea>
                <button name="save" value="yes">Save</button>
                <input value="no-name">
            </form>"#,
        )
        .unwrap();
        let form = find_form_id(&doc).unwrap();
        let pairs = collect_form_data(&doc, form);
        let mut by_name: HashMap<&str, &str> = HashMap::new();
        for (k, v) in &pairs {
            by_name.insert(k.as_str(), v.as_str());
        }
        assert_eq!(by_name.get("q"), Some(&"rust"));
        assert_eq!(by_name.get("hidden"), Some(&"x"));
        assert_eq!(by_name.get("remember"), Some(&"on"));
        assert!(!by_name.contains_key("no"));
        assert_eq!(by_name.get("pick"), Some(&"b"));
        assert_eq!(by_name.get("fruit"), Some(&"banana"));
        assert_eq!(by_name.get("note"), Some(&"hello world"));
        assert_eq!(by_name.get("save"), Some(&"yes"));
        assert_eq!(pairs.len(), 7);
    }

    #[test]
    fn form_action_and_method_defaults() {
        let doc = parse_document(r#"<form></form>"#).unwrap();
        let form = find_form_id(&doc).unwrap();
        assert_eq!(form_action(&doc, form), None);
        assert_eq!(form_method(&doc, form), "get");
    }

    #[test]
    fn urlencode_percent_encodes() {
        let out = urlencode(&[
            ("q".to_string(), "hello world".to_string()),
            ("k".to_string(), "caf\u{e9}".to_string()),
            ("a+b".to_string(), "x&y".to_string()),
        ]);
        assert_eq!(out, "q=hello+world&k=caf%C3%A9&a%2Bb=x%26y");
    }

    #[derive(Clone)]
    struct MockFetcher {
        responses: Arc<HashMap<String, String>>,
        requests: Arc<std::sync::Mutex<Vec<(String, Method, Option<String>)>>>,
    }

    impl MockFetcher {
        fn new(responses: &[(&str, &str)]) -> Self {
            let map = responses
                .iter()
                .map(|(u, b)| (u.to_string(), b.to_string()))
                .collect();
            Self {
                responses: Arc::new(map),
                requests: Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }
    }

    impl kore_net::Fetcher for MockFetcher {
        fn fetch(&self, request: FetchRequest) -> kore_net::BoxedFetch<'_> {
            let url = request.url.as_str().to_string();
            let method = request.method.clone();
            let body = request
                .body
                .clone()
                .map(|b| String::from_utf8_lossy(&b).to_string());
            let responses = self.responses.clone();
            let requests = self.requests.clone();
            Box::pin(async move {
                requests
                    .lock()
                    .unwrap()
                    .push((url.clone(), method, body));
                match responses.get(&url) {
                    Some(html) => Ok(kore_net::FetchResponse {
                        status: 200,
                        final_url: request.url,
                        headers: vec![],
                        body: bytes::Bytes::from(html.clone().into_bytes()),
                    }),
                    None => Err(format!("no mock response for {url}")),
                }
            })
        }
    }

    #[test]
    fn iframe_srcdoc_renders_nested_content() {
        let document = parse_document(
            r#"<iframe srcdoc="<p>nested text</p>" width="320" height="200"></iframe>"#,
        )
        .unwrap();
        let stylesheet = parse_stylesheet(DEFAULT_CSS).unwrap();
        let tree = layout_document(
            &document,
            &stylesheet,
            LayoutConfig {
                viewport_width: 800.0,
                viewport_height: 600.0,
            },
        )
        .unwrap();
        let base = Url::parse("https://example.com/").unwrap();
        let pipeline = Pipeline::default();
        let frames = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(pipeline.render_iframes(&document, &tree, &base, 0));

        assert_eq!(frames.len(), 1, "srcdoc iframe should produce a nested frame");
        let frame = frames.values().next().unwrap();
        assert!(
            find_text(&frame.display_list)
                .iter()
                .any(|t| t.text.contains("nested text")),
            "nested frame should contain the srcdoc text"
        );

        let dl = build_display_list_with_iframes(
            &document,
            &tree,
            &HashMap::new(),
            &base,
            &frames,
        );
        assert!(
            dl.commands()
                .iter()
                .any(|c| matches!(c, kore_gpu::DisplayCommand::PushClip(_))),
            "iframe content should be clipped"
        );
        let nested_text = find_text(&dl)
            .into_iter()
            .find(|t| t.text.contains("nested text"))
            .expect("nested text should be merged into the parent list");
        assert!(
            nested_text.y > frame.y,
            "nested text should be translated inside the frame box"
        );
    }

    #[test]
    fn iframe_src_fetches_and_renders() {
        let mock = MockFetcher::new(&[(
            "https://example.com/inner.html",
            "<p>inner page</p>",
        )]);
        let pipeline = Pipeline::new(Arc::new(mock.clone()));
        let document = parse_document(
            r#"<iframe src="/inner.html" width="320" height="200"></iframe>"#,
        )
        .unwrap();
        let stylesheet = parse_stylesheet(DEFAULT_CSS).unwrap();
        let tree = layout_document(
            &document,
            &stylesheet,
            LayoutConfig {
                viewport_width: 800.0,
                viewport_height: 600.0,
            },
        )
        .unwrap();
        let base = Url::parse("https://example.com/").unwrap();
        let frames = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(pipeline.render_iframes(&document, &tree, &base, 0));
        assert_eq!(frames.len(), 1, "src iframe should be fetched and rendered");
        let frame = frames.values().next().unwrap();
        assert!(
            find_text(&frame.display_list)
                .iter()
                .any(|t| t.text.contains("inner page")),
            "nested frame should contain the fetched page text"
        );
        let recorded = mock.requests.lock().unwrap();
        assert!(
            recorded
                .iter()
                .any(|(u, _, _)| u == "https://example.com/inner.html"),
            "the iframe src should be fetched"
        );
    }

    #[test]
    fn submit_form_get_navigates_with_query() {
        let mock = MockFetcher::new(&[(
            "https://example.com/search?q=hello+world",
            "<title>Results</title><p>found it</p>",
        )]);
        let pipeline = Pipeline::new(Arc::new(mock.clone()));
        let document = parse_document(
            r#"<form action="/search">
                <input name="q" value="hello world">
            </form>"#,
        )
        .unwrap();
        let form = find_form_id(&document).unwrap();
        let base = Url::parse("https://example.com/").unwrap();
        let output = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(pipeline.submit_form(&document, form, &base))
            .unwrap();
        assert_eq!(output.title.as_deref(), Some("Results"));
        assert!(
            find_text(&output.display_list)
                .iter()
                .any(|t| t.text.contains("found it")),
            "GET submit should render the destination page"
        );
    }

    #[test]
    fn submit_form_post_sends_urlencoded_body() {
        let mock = MockFetcher::new(&[(
            "https://example.com/login",
            "<title>Logged In</title><p>welcome</p>",
        )]);
        let pipeline = Pipeline::new(Arc::new(mock.clone()));
        let document = parse_document(
            r#"<form action="/login" method="post">
                <input name="user" value="alice">
                <textarea name="note">hello world</textarea>
            </form>"#,
        )
        .unwrap();
        let form = find_form_id(&document).unwrap();
        let base = Url::parse("https://example.com/").unwrap();
        let output = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(pipeline.submit_form(&document, form, &base))
            .unwrap();
        assert_eq!(output.title.as_deref(), Some("Logged In"));

        let recorded = mock.requests.lock().unwrap();
        let post = recorded
            .iter()
            .find(|(_, m, _)| *m == Method::Post)
            .expect("a POST request should be issued");
        assert_eq!(post.0, "https://example.com/login");
        assert_eq!(
            post.2.as_deref(),
            Some("user=alice&note=hello+world"),
            "POST body should be urlencoded"
        );
    }

    #[test]
    fn tracker_image_is_skipped_and_logged() {
        let mock = MockFetcher::new(&[(
            "https://example.com/page",
            r#"<img src="https://google-analytics.com/ga.gif">"#,
        )]);
        let pipeline = Pipeline::new(Arc::new(mock.clone()));
        let url = Url::parse("https://example.com/page").unwrap();
        let output = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(pipeline.render(&url))
            .unwrap();
        let recorded = mock.requests.lock().unwrap();
        assert!(
            recorded
                .iter()
                .all(|(u, _, _)| u == "https://example.com/page"),
            "the tracker image must not be fetched: {recorded:?}"
        );
        assert_eq!(pipeline.tracking().blocked_count(), 1);
        let blocked = pipeline.tracking().blocked();
        assert_eq!(blocked[0].url, "https://google-analytics.com/ga.gif");
        assert_eq!(
            blocked[0].category,
            Some(kore_net::TrackerCategory::Analytics)
        );
        let gray = Color::from_rgba8(200, 200, 200, 255);
        let has_placeholder = output.display_list.commands().iter().any(|cmd| {
            if let kore_gpu::DisplayCommand::Rect(r) = cmd {
                (r.color.r - gray.r).abs() < 0.01
                    && (r.color.g - gray.g).abs() < 0.01
                    && (r.color.b - gray.b).abs() < 0.01
            } else {
                false
            }
        });
        assert!(has_placeholder, "blocked image should fall back to a placeholder");
    }

    #[test]
    fn tracker_iframe_is_not_rendered() {
        let mock = MockFetcher::new(&[(
            "https://example.com/page",
            r#"<iframe src="https://doubleclick.net/pixel" width="200" height="100"></iframe>"#,
        )]);
        let pipeline = Pipeline::new(Arc::new(mock.clone()));
        let url = Url::parse("https://example.com/page").unwrap();
        let output = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(pipeline.render(&url))
            .unwrap();
        let recorded = mock.requests.lock().unwrap();
        assert!(
            recorded
                .iter()
                .all(|(u, _, _)| u == "https://example.com/page"),
            "the tracker iframe must not be fetched: {recorded:?}"
        );
        assert_eq!(pipeline.tracking().blocked_count(), 1);
        let texts = find_text(&output.display_list);
        assert!(
            texts.iter().any(|t| t.text.contains("iframe")),
            "blocked iframe should show its placeholder box"
        );
    }

    #[test]
    fn etp_disabled_allows_tracker_requests() {
        let mock = MockFetcher::new(&[(
            "https://example.com/page",
            r#"<img src="https://google-analytics.com/ga.gif">"#,
        )]);
        let pipeline = Pipeline::new(Arc::new(mock.clone()));
        pipeline.set_etp_enabled(false);
        let url = Url::parse("https://example.com/page").unwrap();
        let _ = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(pipeline.render(&url))
            .unwrap();
        let recorded = mock.requests.lock().unwrap();
        assert!(
            recorded
                .iter()
                .any(|(u, _, _)| u == "https://google-analytics.com/ga.gif"),
            "with ETP disabled the tracker should be fetched"
        );
        assert_eq!(pipeline.tracking().blocked_count(), 0);
    }
}
