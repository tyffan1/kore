use crate::{
    AlignItems, ComputedStyle, Display, FlexDirection, FlexWrap, FontStyle, FontWeight,
    JustifyContent, LayoutNode, LayoutNodeId, LayoutTree, Rect,
};
use kore_css::{cascade_for_element, ElementSnapshot, StyleSheet};
use kore_html::{Document, Element, NodeId, NodeKind};
use thiserror::Error;

const TEXT_ADVANCE: f32 = 8.0;
const LINE_HEIGHT: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConfig {
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            viewport_width: 800.0,
            viewport_height: 600.0,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LayoutError {
    #[error("DOM node was not found")]
    MissingDomNode,
    #[error("layout tree root was not found")]
    MissingLayoutRoot,
}

pub fn layout_document(
    document: &Document,
    stylesheet: &StyleSheet,
    config: LayoutConfig,
) -> Result<LayoutTree, LayoutError> {
    let mut builder = LayoutBuilder::new(document, stylesheet);
    let root = builder.build_document()?;
    let mut tree = LayoutTree {
        root,
        nodes: builder.nodes,
    };
    layout_node(
        &mut tree.nodes,
        root,
        0.0,
        0.0,
        config.viewport_width,
        config.viewport_height,
    )?;
    Ok(tree)
}

struct LayoutBuilder<'a> {
    document: &'a Document,
    stylesheet: &'a StyleSheet,
    nodes: Vec<LayoutNode>,
}

impl<'a> LayoutBuilder<'a> {
    fn new(document: &'a Document, stylesheet: &'a StyleSheet) -> Self {
        Self {
            document,
            stylesheet,
            nodes: Vec::new(),
        }
    }

    fn build_document(&mut self) -> Result<LayoutNodeId, LayoutError> {
        let root = self.push_node(
            None,
            None,
            ComputedStyle {
                display: Display::Block,
                ..ComputedStyle::default()
            },
        );
        let dom_root = self
            .document
            .node(self.document.root())
            .ok_or(LayoutError::MissingDomNode)?;
        for child_id in &dom_root.children {
            self.build_dom_subtree(*child_id, root)?;
        }
        Ok(root)
    }

    fn build_dom_subtree(
        &mut self,
        dom_id: NodeId,
        parent: LayoutNodeId,
    ) -> Result<(), LayoutError> {
        let dom_node = self
            .document
            .node(dom_id)
            .ok_or(LayoutError::MissingDomNode)?;
        match &dom_node.kind {
            NodeKind::Element(element) => {
                let mut style = self.computed_style(element);
                if style.display == Display::None {
                    return Ok(());
                }
                // Inherit from parent layout node
                inherit_from_parent(&mut style, &self.nodes, parent);
                let layout_id = self.push_node(Some(dom_id), Some(parent), style);
                for child_id in &dom_node.children {
                    self.build_dom_subtree(*child_id, layout_id)?;
                }
            }
            NodeKind::Text(text) if !text.trim().is_empty() => {
                let mut style = ComputedStyle {
                    display: Display::Inline,
                    width: Some(text_width(text)),
                    height: Some(LINE_HEIGHT),
                    ..ComputedStyle::default()
                };
                inherit_from_parent(&mut style, &self.nodes, parent);
                self.push_node(Some(dom_id), Some(parent), style);
            }
            NodeKind::Document
            | NodeKind::Doctype(_)
            | NodeKind::Comment(_)
            | NodeKind::Text(_) => {}
        }
        Ok(())
    }

    fn computed_style(&self, element: &Element) -> ComputedStyle {
        let snapshot = element_snapshot(element);
        let properties = cascade_for_element(self.stylesheet, &snapshot);
        ComputedStyle::from_cascade(&properties, default_display(&element.tag_name))
    }

    fn push_node(
        &mut self,
        dom_node_id: Option<NodeId>,
        parent: Option<LayoutNodeId>,
        style: ComputedStyle,
    ) -> LayoutNodeId {
        let id = LayoutNodeId(self.nodes.len());
        self.nodes.push(LayoutNode {
            id,
            dom_node_id,
            parent,
            children: Vec::new(),
            style,
            rect: Rect::ZERO,
        });
        if let Some(parent) = parent {
            if let Some(parent_node) = self.nodes.get_mut(parent.0) {
                parent_node.children.push(id);
            }
        }
        id
    }
}

fn layout_node(
    nodes: &mut [LayoutNode],
    id: LayoutNodeId,
    x: f32,
    y: f32,
    containing_width: f32,
    containing_height: f32,
) -> Result<f32, LayoutError> {
    let display = nodes
        .get(id.0)
        .ok_or(LayoutError::MissingLayoutRoot)?
        .style
        .display;
    match display {
        Display::Flex => layout_flex(nodes, id, x, y, containing_width, containing_height),
        Display::Inline | Display::InlineBlock => {
            let width = preferred_width(&nodes[id.0], containing_width);
            let height = preferred_height(&nodes[id.0], LINE_HEIGHT);
            // Lay out children as inline content
            let children = nodes[id.0].children.clone();
            let mut cursor_x = x;
            let mut max_h = height;
            for child in children {
                let child_w = preferred_width(&nodes[child.0], width);
                let child_h = preferred_height(&nodes[child.0], LINE_HEIGHT);
                let _ = layout_node(nodes, child, cursor_x, y, child_w, child_h);
                cursor_x += child_w;
                max_h = max_h.max(child_h);
            }
            nodes[id.0].rect = Rect::new(x, y, (cursor_x - x).max(width), max_h);
            Ok(max_h)
        }
        Display::Block => layout_block(nodes, id, x, y, containing_width, containing_height),
        Display::None => Ok(0.0),
    }
}

fn layout_block(
    nodes: &mut [LayoutNode],
    id: LayoutNodeId,
    x: f32,
    y: f32,
    containing_width: f32,
    containing_height: f32,
) -> Result<f32, LayoutError> {
    let style = nodes[id.0].style.clone();
    let width = preferred_width(&nodes[id.0], containing_width);
    let content_x = x + style.border.left + style.padding.left;
    let content_y = y + style.border.top + style.padding.top;
    let content_width = style.content_width(width);
    let children = nodes[id.0].children.clone();
    let mut cursor_y = content_y;
    let mut line_x = content_x;
    let mut line_y = content_y;
    let mut line_height = 0.0;

    for child in children {
        let child_display = nodes[child.0].style.display;
        if matches!(child_display, Display::Inline | Display::InlineBlock) {
            let child_width = preferred_width(&nodes[child.0], content_width);
            let child_height = preferred_height(&nodes[child.0], LINE_HEIGHT);
            if line_x > content_x && line_x + child_width > content_x + content_width {
                cursor_y += line_height;
                line_y = cursor_y;
                line_x = content_x;
                line_height = 0.0;
            }
            layout_node(nodes, child, line_x, line_y, child_width, child_height)?;
            line_x += child_width;
            line_height = line_height.max(child_height);
        } else {
            if line_height > 0.0 {
                cursor_y += line_height;
                line_x = content_x;
                line_y = cursor_y;
                line_height = 0.0;
            }
            let margin = nodes[child.0].style.margin;
            cursor_y += margin.top;
            let child_x = content_x + margin.left;
            let child_width = (content_width - margin.horizontal()).max(0.0);
            let child_height = layout_node(
                nodes,
                child,
                child_x,
                cursor_y,
                child_width,
                containing_height,
            )?;
            cursor_y += child_height + margin.bottom;
        }
    }

    if line_height > 0.0 {
        cursor_y += line_height;
    }

    let content_height = nodes[id.0]
        .style
        .height
        .unwrap_or((cursor_y - content_y).max(0.0));
    let height = content_height + style.padding.vertical() + style.border.vertical();
    nodes[id.0].rect = Rect::new(x, y, width, height);
    Ok(height)
}

fn layout_flex(
    nodes: &mut [LayoutNode],
    id: LayoutNodeId,
    x: f32,
    y: f32,
    containing_width: f32,
    containing_height: f32,
) -> Result<f32, LayoutError> {
    let style = nodes[id.0].style.clone();
    let width = preferred_width(&nodes[id.0], containing_width);
    let content_width = style.content_width(width);
    let content_x = x + style.border.left + style.padding.left;
    let content_y = y + style.border.top + style.padding.top;
    let children = nodes[id.0].children.clone();

    let mut items = children
        .iter()
        .map(|child| {
            let child_style = &nodes[child.0].style;
            let width = preferred_width(&nodes[child.0], content_width);
            let height = preferred_height(&nodes[child.0], LINE_HEIGHT);
            (
                *child,
                width + child_style.margin.horizontal(),
                height + child_style.margin.vertical(),
            )
        })
        .collect::<Vec<_>>();

    if style.flex_wrap == FlexWrap::Wrap && style.flex_direction == FlexDirection::Row {
        wrap_row_items(&mut items, content_width);
    }

    let content_height = style.height.unwrap_or_else(|| match style.flex_direction {
        FlexDirection::Row => items
            .iter()
            .map(|(_, _, height)| *height)
            .fold(0.0_f32, f32::max),
        FlexDirection::Column => items.iter().map(|(_, _, height)| *height).sum(),
    });

    let context = FlexLayoutContext {
        x: content_x,
        y: content_y,
        width: content_width,
        height: content_height,
        align_items: style.align_items,
        justify_content: style.justify_content,
    };

    match style.flex_direction {
        FlexDirection::Row => layout_flex_row(nodes, &items, &context)?,
        FlexDirection::Column => layout_flex_column(nodes, &items, &context)?,
    }

    let height = content_height + style.padding.vertical() + style.border.vertical();
    nodes[id.0].rect = Rect::new(x, y, width, height);
    let _ = containing_height;
    Ok(height)
}

struct FlexLayoutContext {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    align_items: AlignItems,
    justify_content: JustifyContent,
}

fn layout_flex_row(
    nodes: &mut [LayoutNode],
    items: &[(LayoutNodeId, f32, f32)],
    context: &FlexLayoutContext,
) -> Result<(), LayoutError> {
    let total_width = items.iter().map(|(_, width, _)| *width).sum::<f32>();
    let line_height = items
        .iter()
        .map(|(_, _, height)| *height)
        .fold(0.0_f32, f32::max);
    let (mut cursor_x, gap) = flex_offset_and_gap(
        context.justify_content,
        context.x,
        context.width,
        total_width,
        items.len(),
    );
    for (child, outer_width, outer_height) in items {
        let margin = nodes[child.0].style.margin;
        let child_width = (outer_width - margin.horizontal()).max(0.0);
        let child_height = (outer_height - margin.vertical()).max(0.0);
        let cross_offset = cross_axis_offset(
            context.align_items,
            context.height.max(line_height),
            *outer_height,
        );
        layout_node(
            nodes,
            *child,
            cursor_x + margin.left,
            context.y + cross_offset + margin.top,
            child_width,
            child_height,
        )?;
        cursor_x += outer_width + gap;
    }
    Ok(())
}

fn layout_flex_column(
    nodes: &mut [LayoutNode],
    items: &[(LayoutNodeId, f32, f32)],
    context: &FlexLayoutContext,
) -> Result<(), LayoutError> {
    let total_height = items.iter().map(|(_, _, height)| *height).sum::<f32>();
    let (mut cursor_y, gap) = flex_offset_and_gap(
        context.justify_content,
        context.y,
        context.height,
        total_height,
        items.len(),
    );
    for (child, outer_width, outer_height) in items {
        let margin = nodes[child.0].style.margin;
        let child_width = (outer_width - margin.horizontal()).max(0.0);
        let child_height = (outer_height - margin.vertical()).max(0.0);
        let cross_offset = cross_axis_offset(context.align_items, context.width, *outer_width);
        layout_node(
            nodes,
            *child,
            context.x + cross_offset + margin.left,
            cursor_y + margin.top,
            child_width,
            child_height,
        )?;
        cursor_y += outer_height + gap;
    }
    Ok(())
}

fn flex_offset_and_gap(
    justify: JustifyContent,
    origin: f32,
    available: f32,
    used: f32,
    count: usize,
) -> (f32, f32) {
    let free = (available - used).max(0.0);
    match justify {
        JustifyContent::Center => (origin + free / 2.0, 0.0),
        JustifyContent::FlexEnd => (origin + free, 0.0),
        JustifyContent::SpaceBetween if count > 1 => (origin, free / (count as f32 - 1.0)),
        JustifyContent::FlexStart | JustifyContent::SpaceBetween => (origin, 0.0),
    }
}

fn cross_axis_offset(align: AlignItems, available: f32, used: f32) -> f32 {
    let free = (available - used).max(0.0);
    match align {
        AlignItems::Center => free / 2.0,
        AlignItems::FlexEnd => free,
        AlignItems::FlexStart | AlignItems::Stretch => 0.0,
    }
}

fn wrap_row_items(items: &mut [(LayoutNodeId, f32, f32)], content_width: f32) {
    let mut line_width = 0.0;
    let mut line_offset = 0.0;
    for (_, width, height) in items {
        if line_width > 0.0 && line_width + *width > content_width {
            line_offset += *height;
            line_width = 0.0;
        }
        *height += line_offset;
        line_width += *width;
    }
}

fn preferred_width(node: &LayoutNode, containing_width: f32) -> f32 {
    let style = &node.style;
    let base = style.width.unwrap_or(containing_width.max(0.0));
    base + style.padding.horizontal() + style.border.horizontal()
}

fn preferred_height(node: &LayoutNode, fallback: f32) -> f32 {
    let style = &node.style;
    style.height.unwrap_or(fallback) + style.padding.vertical() + style.border.vertical()
}

fn element_snapshot(element: &Element) -> ElementSnapshot {
    let mut snapshot = ElementSnapshot::new(&element.tag_name);
    for attr in &element.attributes {
        match attr.name.as_str() {
            "id" => snapshot.id = Some(attr.value.clone()),
            "class" => {
                snapshot
                    .classes
                    .extend(attr.value.split_whitespace().map(str::to_string));
            }
            name => snapshot.attributes.push(name.to_ascii_lowercase()),
        }
    }
    snapshot
}

fn default_display(tag_name: &str) -> Display {
    match tag_name {
        "a" | "b" | "button" | "em" | "i" | "label" | "span" | "strong" => Display::Inline,
        "script" | "style" | "template" => Display::None,
        _ => Display::Block,
    }
}

fn text_width(text: &str) -> f32 {
    text.chars().count() as f32 * TEXT_ADVANCE
}

/// Inherit CSS properties that cascade by default from the parent layout node.
fn inherit_from_parent(style: &mut ComputedStyle, nodes: &[LayoutNode], parent: LayoutNodeId) {
    let Some(p) = nodes.get(parent.0).map(|n| &n.style) else {
        return;
    };
    if style.color.is_none() {
        style.color = p.color;
    }
    if style.font_size.is_none() {
        style.font_size = p.font_size;
    }
    if style.font_weight == FontWeight::Normal && p.font_weight == FontWeight::Bold {
        style.font_weight = p.font_weight;
    }
    if style.font_style == FontStyle::Normal && p.font_style == FontStyle::Italic {
        style.font_style = p.font_style;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kore_css::{parse_stylesheet, ParserError};
    use kore_html::{parse_document, TokenizerError};

    #[derive(Debug, Error)]
    enum TestError {
        #[error(transparent)]
        Html(#[from] TokenizerError),
        #[error(transparent)]
        Css(#[from] ParserError),
        #[error(transparent)]
        Layout(#[from] LayoutError),
        #[error("layout node with DOM id `{0}` was not found")]
        MissingNode(String),
    }

    fn render(html: &str, css: &str) -> Result<(Document, LayoutTree), TestError> {
        let document = parse_document(html)?;
        let stylesheet = parse_stylesheet(css)?;
        let tree = layout_document(
            &document,
            &stylesheet,
            LayoutConfig {
                viewport_width: 800.0,
                viewport_height: 600.0,
            },
        )?;
        Ok((document, tree))
    }

    fn node_by_dom_id<'a>(
        document: &Document,
        tree: &'a LayoutTree,
        expected_id: &str,
    ) -> Result<&'a LayoutNode, TestError> {
        let dom_node = document
            .nodes()
            .iter()
            .find(|node| match &node.kind {
                NodeKind::Element(element) => element
                    .attributes
                    .iter()
                    .any(|attr| attr.name == "id" && attr.value == expected_id),
                _ => false,
            })
            .ok_or_else(|| TestError::MissingNode(expected_id.to_string()))?;

        tree.nodes
            .iter()
            .find(|node| node.dom_node_id == Some(dom_node.id))
            .ok_or_else(|| TestError::MissingNode(expected_id.to_string()))
    }

    #[test]
    fn computes_block_layout_with_box_model() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="parent"><div id="a"></div><div id="b"></div></div>"#,
            r#"
                #parent { width: 100px; padding: 10px; border: 2px; }
                #a { height: 20px; margin: 5px; }
                #b { height: 10px; }
            "#,
        )?;

        let parent = node_by_dom_id(&document, &tree, "parent")?;
        let first = node_by_dom_id(&document, &tree, "a")?;
        let second = node_by_dom_id(&document, &tree, "b")?;

        assert_eq!(parent.rect, Rect::new(0.0, 0.0, 124.0, 64.0));
        assert_eq!(first.rect, Rect::new(17.0, 17.0, 90.0, 20.0));
        assert_eq!(second.rect, Rect::new(12.0, 42.0, 100.0, 10.0));
        Ok(())
    }

    #[test]
    fn lays_out_inline_flow_with_wrapping() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="root"><span id="a"></span><span id="b"></span><span id="c"></span></div>"#,
            r#"
                #root { width: 100px; }
                span { display: inline-block; width: 40px; height: 10px; }
            "#,
        )?;

        let first = node_by_dom_id(&document, &tree, "a")?;
        let second = node_by_dom_id(&document, &tree, "b")?;
        let third = node_by_dom_id(&document, &tree, "c")?;

        assert_eq!(first.rect, Rect::new(0.0, 0.0, 40.0, 10.0));
        assert_eq!(second.rect, Rect::new(40.0, 0.0, 40.0, 10.0));
        assert_eq!(third.rect, Rect::new(0.0, 10.0, 40.0, 10.0));
        Ok(())
    }

    #[test]
    fn lays_out_flexbox_row() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="flex"><div id="a"></div><div id="b"></div></div>"#,
            r#"
                #flex {
                    display: flex;
                    width: 200px;
                    height: 50px;
                    justify-content: center;
                    align-items: center;
                }
                #a, #b { width: 50px; height: 10px; }
            "#,
        )?;

        let first = node_by_dom_id(&document, &tree, "a")?;
        let second = node_by_dom_id(&document, &tree, "b")?;

        assert_eq!(first.rect, Rect::new(50.0, 20.0, 50.0, 10.0));
        assert_eq!(second.rect, Rect::new(100.0, 20.0, 50.0, 10.0));
        Ok(())
    }

    #[test]
    fn lays_out_flexbox_column() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="flex"><div id="a"></div><div id="b"></div></div>"#,
            r#"
                #flex {
                    display: flex;
                    flex-direction: column;
                    width: 100px;
                    height: 100px;
                    justify-content: space-between;
                    align-items: center;
                }
                #a, #b { width: 20px; height: 10px; }
            "#,
        )?;

        let first = node_by_dom_id(&document, &tree, "a")?;
        let second = node_by_dom_id(&document, &tree, "b")?;

        assert_eq!(first.rect, Rect::new(40.0, 0.0, 20.0, 10.0));
        assert_eq!(second.rect, Rect::new(40.0, 90.0, 20.0, 10.0));
        Ok(())
    }
}
