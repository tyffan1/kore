use crate::{
    AlignItems, ComputedStyle, Display, FlexDirection, FlexWrap, FontStyle, FontWeight,
    GridTrack, JustifyContent, LayoutNode, LayoutNodeId, LayoutTree, Position, Rect,
};
use kore_css::{cascade_for_element, ElementSnapshot, StyleSheet};
use kore_html::{Document, Element, NodeId, NodeKind};
use thiserror::Error;

pub(crate) const LINE_HEIGHT: f32 = 16.0;
const TEXT_ADVANCE_FACTOR: f32 = 0.6;

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
    apply_relative_offsets(&mut tree.nodes, root);
    position_out_of_flow(&mut tree.nodes, config)?;
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
                // Replaced elements: size from attributes or defaults
                let tag = element.tag_name.to_ascii_lowercase();
                if matches!(tag.as_str(), "img" | "video" | "audio" | "iframe") {
                    let (default_w, default_h) = match tag.as_str() {
                        "audio" => (300.0, 54.0),
                        "video" | "iframe" => (300.0, 150.0),
                        _ => (100.0, 100.0),
                    };
                    if style.width.is_none() {
                        style.width = element
                            .attributes
                            .iter()
                            .find(|a| a.name.eq_ignore_ascii_case("width"))
                            .and_then(|a| a.value.parse::<f32>().ok())
                            .or(Some(default_w));
                    }
                    if style.height.is_none() {
                        style.height = element
                            .attributes
                            .iter()
                            .find(|a| a.name.eq_ignore_ascii_case("height"))
                            .and_then(|a| a.value.parse::<f32>().ok())
                            .or(Some(default_h));
                    }
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
                    width: None,
                    height: None,
                    ..ComputedStyle::default()
                };
                inherit_from_parent(&mut style, &self.nodes, parent);
                let font_size = style.font_size.unwrap_or(16.0);
                style.width = Some(text_width(text, font_size));
                style.height = Some(font_size * 1.5);
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
        Display::Grid => layout_grid(nodes, id, x, y, containing_width, containing_height),
        Display::Inline | Display::InlineBlock => {
            // Inline without explicit width: shrink-to-fit (width = 0 so children determine it)
            let width = if matches!(display, Display::Inline) && nodes[id.0].style.width.is_none() {
                0.0
            } else {
                preferred_width(&nodes[id.0], containing_width)
            };
            let height = preferred_height(&nodes[id.0], LINE_HEIGHT);
            // Lay out children as inline content
            let children = nodes[id.0].children.clone();
            let mut cursor_x = x;
            let mut max_h = height;
            for child in children {
                if is_out_of_flow(&nodes[child.0]) {
                    continue;
                }
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
        if is_out_of_flow(&nodes[child.0]) {
            continue;
        }
        let child_display = nodes[child.0].style.display;
        if matches!(child_display, Display::Inline | Display::InlineBlock) {
            let child_width = if matches!(child_display, Display::Inline) && nodes[child.0].style.width.is_none() {
                0.0
            } else {
                preferred_width(&nodes[child.0], content_width)
            };
            let child_height = preferred_height(&nodes[child.0], LINE_HEIGHT);
            if line_x > content_x && line_x + child_width > content_x + content_width {
                cursor_y += line_height;
                line_y = cursor_y;
                line_x = content_x;
                line_height = 0.0;
            }
            layout_node(nodes, child, line_x, line_y, child_width, child_height)?;
            let adv_width = if matches!(child_display, Display::Inline) {
                nodes[child.0].rect.width
            } else {
                child_width
            };
            line_x += adv_width;
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
        .filter(|child| !is_out_of_flow(&nodes[child.0]))
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

fn is_out_of_flow(node: &LayoutNode) -> bool {
    matches!(node.style.position, Position::Absolute | Position::Fixed)
}

/// Shift `position: relative` boxes (and their subtrees) by their offsets.
/// Runs before the absolute/fixed pass so shifted boxes act as containing blocks.
fn apply_relative_offsets(nodes: &mut [LayoutNode], root: LayoutNodeId) {
    let ids = nodes.iter().map(|node| node.id).collect::<Vec<_>>();
    for id in ids {
        let style = nodes[id.0].style.clone();
        if style.position != Position::Relative {
            continue;
        }
        let dx = style.left.unwrap_or(0.0) - style.right.unwrap_or(0.0);
        let dy = style.top.unwrap_or(0.0) - style.bottom.unwrap_or(0.0);
        if dx != 0.0 || dy != 0.0 {
            shift_subtree(nodes, id, dx, dy);
        }
    }
    let _ = root;
}

fn shift_subtree(nodes: &mut [LayoutNode], id: LayoutNodeId, dx: f32, dy: f32) {
    if let Some(node) = nodes.get_mut(id.0) {
        node.rect.x += dx;
        node.rect.y += dy;
        let children = node.children.clone();
        for child in children {
            shift_subtree(nodes, child, dx, dy);
        }
    }
}

/// Position `position: absolute`/`fixed` boxes after the in-flow pass.
/// `fixed` is anchored to the viewport; `absolute` to the nearest positioned
/// ancestor's padding box (or the viewport when none exists).
fn position_out_of_flow(
    nodes: &mut [LayoutNode],
    config: LayoutConfig,
) -> Result<(), LayoutError> {
    let viewport = Rect::new(0.0, 0.0, config.viewport_width, config.viewport_height);
    let ids = nodes.iter().map(|node| node.id).collect::<Vec<_>>();
    for id in ids {
        let position = nodes[id.0].style.position;
        if !matches!(position, Position::Absolute | Position::Fixed) {
            continue;
        }
        let containing = containing_block(nodes, id, viewport);
        let style = nodes[id.0].style.clone();
        let width = style
            .width
            .map(|w| w + style.padding.horizontal() + style.border.horizontal())
            .unwrap_or_else(|| shrink_to_fit_width(nodes, id, containing.width));
        // Measure content height first (offsets like `bottom` need it).
        let _ = layout_node(nodes, id, 0.0, 0.0, width, containing.height)?;
        let measured_height = nodes[id.0].rect.height;
        let height = style
            .height
            .map(|h| h + style.padding.vertical() + style.border.vertical())
            .unwrap_or(measured_height);
        let mut x = containing.x;
        let mut y = containing.y;
        if let Some(left) = style.left {
            x = containing.x + left;
        } else if let Some(right) = style.right {
            x = containing.x + containing.width - width - right;
        }
        if let Some(top) = style.top {
            y = containing.y + top;
        } else if let Some(bottom) = style.bottom {
            y = containing.y + containing.height - height - bottom;
        }
        let _ = layout_node(nodes, id, x, y, width, height)?;
        nodes[id.0].rect = Rect::new(x, y, width, height);
    }
    Ok(())
}

fn containing_block(nodes: &[LayoutNode], id: LayoutNodeId, viewport: Rect) -> Rect {
    if nodes[id.0].style.position == Position::Fixed {
        return viewport;
    }
    let mut current = nodes[id.0].parent;
    while let Some(parent) = current {
        let node = &nodes[parent.0];
        if node.style.position != Position::Static {
            let style = &node.style;
            return Rect::new(
                node.rect.x + style.border.left,
                node.rect.y + style.border.top,
                node.rect.width - style.border.horizontal(),
                node.rect.height - style.border.vertical(),
            );
        }
        current = node.parent;
    }
    viewport
}

fn shrink_to_fit_width(nodes: &[LayoutNode], id: LayoutNodeId, max_width: f32) -> f32 {
    let node = &nodes[id.0];
    let style = &node.style;
    let mut inner: f32 = 0.0;
    for child in &node.children {
        let child_style = &nodes[child.0].style;
        if is_out_of_flow(&nodes[child.0]) {
            continue;
        }
        let child_width = if matches!(child_style.display, Display::Inline)
            && child_style.width.is_none()
        {
            0.0
        } else {
            preferred_width(&nodes[child.0], max_width)
        };
        inner = inner.max(child_width);
    }
    inner + style.padding.horizontal() + style.border.horizontal()
}

fn layout_grid(
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

    let items = children
        .iter()
        .filter(|child| !is_out_of_flow(&nodes[child.0]))
        .copied()
        .collect::<Vec<_>>();

    let columns = if style.grid_columns.is_empty() {
        vec![GridTrack::Fraction(1.0)]
    } else {
        style.grid_columns.clone()
    };
    let column_count = columns.len();
    let col_sizes = resolve_tracks(&columns, content_width, style.column_gap);

    let mut occupied: Vec<Vec<bool>> = Vec::new();
    let mut placements: Vec<(LayoutNodeId, usize, usize, usize, usize)> = Vec::new();
    let mut row_cursor = 0usize;
    for child in items {
        let child_style = &nodes[child.0].style;
        let span_c = (child_style.grid_column_span as usize).max(1).min(column_count);
        let span_r = (child_style.grid_row_span as usize).max(1);
        let start_row = child_style.grid_row_start.map(|s| s as usize).unwrap_or(row_cursor);
        let start_col = child_style.grid_column_start.map(|s| s as usize);
        let (cell_row, cell_col) =
            find_free_cell(&occupied, column_count, start_row, start_col, span_r, span_c);
        mark_cells(&mut occupied, cell_row, cell_col, span_r, span_c, column_count);
        // Explicitly placed items don't move the auto-placement cursor.
        if child_style.grid_row_start.is_none() && child_style.grid_column_start.is_none() {
            row_cursor = if cell_col + span_c >= column_count {
                cell_row + span_r
            } else {
                cell_row
            };
        }
        placements.push((child, cell_row, cell_col, span_r, span_c));
    }

    // Resolve row sizes: explicit tracks, auto rows or content-based heights.
    let row_count = occupied.len();
    let mut row_sizes = vec![0.0f32; row_count];
    let mut fraction_rows: Vec<(usize, f32)> = Vec::new();
    let mut fixed_total = 0.0f32;
    for (row, size) in row_sizes.iter_mut().enumerate() {
        let track = style
            .grid_rows
            .get(row)
            .copied()
            .or_else(|| style.grid_auto_rows.map(GridTrack::Fixed));
        match track {
            Some(GridTrack::Fixed(track_size)) => {
                *size = track_size;
                fixed_total += track_size;
            }
            Some(GridTrack::Fraction(fraction)) => fraction_rows.push((row, fraction)),
            None => {
                let mut max_height = 0.0f32;
                for (child, cell_row, _, span_r, _) in &placements {
                    if *cell_row <= row && row < cell_row + span_r {
                        max_height = max_height.max(preferred_height(&nodes[child.0], LINE_HEIGHT));
                    }
                }
                *size = max_height;
                fixed_total += max_height;
            }
        }
    }
    let gap_total = style.row_gap * row_count.saturating_sub(1) as f32;
    let auto_height = fixed_total + gap_total;
    let content_height = style.height.unwrap_or(auto_height);
    let remaining = (content_height - fixed_total - gap_total).max(0.0);
    let fraction_total = fraction_rows.iter().map(|(_, f)| *f).sum::<f32>();
    if fraction_total > 0.0 {
        for (row, fraction) in fraction_rows {
            row_sizes[row] = remaining * fraction / fraction_total;
        }
    }

    for (child, cell_row, cell_col, span_r, span_c) in placements {
        let mut cell_x = content_x;
        for size in &col_sizes[..cell_col] {
            cell_x += size + style.column_gap;
        }
        let mut cell_y = content_y;
        for size in &row_sizes[..cell_row] {
            cell_y += size + style.row_gap;
        }
        let cell_width = col_sizes[cell_col..cell_col + span_c].iter().sum::<f32>()
            + style.column_gap * span_c.saturating_sub(1) as f32;
        let cell_height = row_sizes[cell_row..cell_row + span_r].iter().sum::<f32>()
            + style.row_gap * span_r.saturating_sub(1) as f32;
        let _ = layout_node(nodes, child, cell_x, cell_y, cell_width, cell_height)?;
        let child_style = nodes[child.0].style.clone();
        let child_rect = &mut nodes[child.0].rect;
        if child_style.width.is_none() {
            child_rect.width = cell_width;
        }
        if child_style.height.is_none() {
            child_rect.height = cell_height;
        }
    }

    let height = content_height + style.padding.vertical() + style.border.vertical();
    nodes[id.0].rect = Rect::new(x, y, width, height);
    let _ = containing_height;
    Ok(height)
}

/// Resolve track definitions against the available space. Fixed tracks keep
/// their size; `fr` tracks share the remaining space proportionally.
fn resolve_tracks(tracks: &[GridTrack], total: f32, gap: f32) -> Vec<f32> {
    let gap_total = gap * tracks.len().saturating_sub(1) as f32;
    let fixed_total = tracks
        .iter()
        .map(|track| match track {
            GridTrack::Fixed(size) => *size,
            GridTrack::Fraction(_) => 0.0,
        })
        .sum::<f32>();
    let fraction_total = tracks
        .iter()
        .map(|track| match track {
            GridTrack::Fraction(fraction) => *fraction,
            GridTrack::Fixed(_) => 0.0,
        })
        .sum::<f32>();
    let remaining = (total - fixed_total - gap_total).max(0.0);
    tracks
        .iter()
        .map(|track| match track {
            GridTrack::Fixed(size) => *size,
            GridTrack::Fraction(fraction) => {
                if fraction_total > 0.0 {
                    remaining * fraction / fraction_total
                } else {
                    0.0
                }
            }
        })
        .collect()
}

/// Find the next free cell for auto-placement (row-major), honoring explicit
/// start lines and spans.
fn find_free_cell(
    occupied: &[Vec<bool>],
    column_count: usize,
    start_row: usize,
    start_col: Option<usize>,
    span_r: usize,
    span_c: usize,
) -> (usize, usize) {
    let mut row = start_row;
    loop {
        match start_col {
            Some(col) => {
                if col + span_c <= column_count
                    && grid_cell_free(occupied, row, col, span_r, span_c)
                {
                    return (row, col);
                }
            }
            None => {
                for col in 0..column_count {
                    if col + span_c <= column_count
                        && grid_cell_free(occupied, row, col, span_r, span_c)
                    {
                        return (row, col);
                    }
                }
            }
        }
        row += 1;
    }
}

fn grid_cell_free(occupied: &[Vec<bool>], row: usize, col: usize, span_r: usize, span_c: usize) -> bool {
    // Rows beyond the currently occupied ones are always free.
    let available = occupied.len().min(row + span_r);
    for occupied_row in &occupied[row..available] {
        if occupied_row[col..col + span_c].iter().any(|cell| *cell) {
            return false;
        }
    }
    true
}

fn mark_cells(
    occupied: &mut Vec<Vec<bool>>,
    row: usize,
    col: usize,
    span_r: usize,
    span_c: usize,
    column_count: usize,
) {
    let needed_rows = row + span_r;
    while occupied.len() < needed_rows {
        occupied.push(vec![false; column_count]);
    }
    for occupied_row in &mut occupied[row..row + span_r] {
        for cell in &mut occupied_row[col..col + span_c] {
            *cell = true;
        }
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

fn text_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * TEXT_ADVANCE_FACTOR
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
    fn replaced_elements_get_default_sizes() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="root">
                <video id="v"></video>
                <audio id="a"></audio>
                <iframe id="f"></iframe>
                <img id="i" width="40" height="30">
            </div>"#,
            "",
        )?;

        let video = node_by_dom_id(&document, &tree, "v")?;
        let audio = node_by_dom_id(&document, &tree, "a")?;
        let iframe = node_by_dom_id(&document, &tree, "f")?;
        let img = node_by_dom_id(&document, &tree, "i")?;

        assert_eq!(video.rect.width, 300.0);
        assert_eq!(video.rect.height, 150.0);
        assert_eq!(audio.rect.width, 300.0);
        assert_eq!(audio.rect.height, 54.0);
        assert_eq!(iframe.rect.width, 300.0);
        assert_eq!(iframe.rect.height, 150.0);
        assert_eq!(img.rect.width, 40.0);
        assert_eq!(img.rect.height, 30.0);
        Ok(())
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

    #[test]
    fn positions_absolute_within_relative_parent() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="parent"><div id="abs"></div><div id="sibling"></div></div>"#,
            r#"
                #parent { position: relative; width: 200px; height: 100px; padding: 10px; }
                #abs { position: absolute; top: 5px; left: 5px; width: 20px; height: 10px; }
                #sibling { height: 10px; }
            "#,
        )?;

        let parent = node_by_dom_id(&document, &tree, "parent")?;
        let abs = node_by_dom_id(&document, &tree, "abs")?;
        let sibling = node_by_dom_id(&document, &tree, "sibling")?;

        assert_eq!(parent.rect, Rect::new(0.0, 0.0, 220.0, 120.0));
        assert_eq!(abs.rect, Rect::new(5.0, 5.0, 20.0, 10.0));
        assert_eq!(sibling.rect, Rect::new(10.0, 10.0, 200.0, 10.0));
        Ok(())
    }

    #[test]
    fn positions_absolute_against_viewport_without_positioned_ancestor() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="abs"></div>"#,
            r#"
                #abs { position: absolute; top: 30px; left: 40px; width: 50px; height: 20px; }
            "#,
        )?;

        let abs = node_by_dom_id(&document, &tree, "abs")?;
        assert_eq!(abs.rect, Rect::new(40.0, 30.0, 50.0, 20.0));
        Ok(())
    }

    #[test]
    fn positions_fixed_against_viewport_ignoring_parent() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="parent"><div id="fix"></div></div>"#,
            r#"
                #parent { position: relative; width: 300px; height: 200px; }
                #fix { position: fixed; top: 10px; left: 15px; width: 30px; height: 10px; }
            "#,
        )?;

        let fix = node_by_dom_id(&document, &tree, "fix")?;
        assert_eq!(fix.rect, Rect::new(15.0, 10.0, 30.0, 10.0));
        Ok(())
    }

    #[test]
    fn positions_absolute_with_right_and_bottom_offsets() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="parent"><div id="abs"></div></div>"#,
            r#"
                #parent { position: relative; width: 200px; height: 100px; }
                #abs { position: absolute; right: 10px; bottom: 5px; width: 40px; height: 20px; }
            "#,
        )?;

        let abs = node_by_dom_id(&document, &tree, "abs")?;
        assert_eq!(abs.rect, Rect::new(150.0, 75.0, 40.0, 20.0));
        Ok(())
    }

    #[test]
    fn sizes_absolute_with_auto_dimensions_from_content() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="abs"><div id="content"></div></div>"#,
            r#"
                #abs { position: absolute; top: 5px; left: 5px; }
                #content { width: 30px; height: 10px; }
            "#,
        )?;

        let abs = node_by_dom_id(&document, &tree, "abs")?;
        let content = node_by_dom_id(&document, &tree, "content")?;
        assert_eq!(abs.rect, Rect::new(5.0, 5.0, 30.0, 10.0));
        assert_eq!(content.rect, Rect::new(5.0, 5.0, 30.0, 10.0));
        Ok(())
    }

    #[test]
    fn keeps_sticky_in_normal_flow() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="parent"><div id="st"></div><div id="b"></div></div>"#,
            r#"
                #parent { width: 100px; }
                #st { position: sticky; top: 5px; height: 10px; }
                #b { height: 10px; }
            "#,
        )?;

        let st = node_by_dom_id(&document, &tree, "st")?;
        let b = node_by_dom_id(&document, &tree, "b")?;
        assert_eq!(st.style.position, Position::Sticky);
        assert_eq!(st.rect, Rect::new(0.0, 0.0, 100.0, 10.0));
        assert_eq!(b.rect, Rect::new(0.0, 10.0, 100.0, 10.0));
        Ok(())
    }

    #[test]
    fn shifts_relative_element_and_its_children() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="parent"><div id="rel"><div id="inner"></div></div><div id="after"></div></div>"#,
            r#"
                #parent { width: 100px; }
                #rel { position: relative; top: 10px; left: 5px; height: 20px; }
                #inner { height: 5px; }
                #after { height: 10px; }
            "#,
        )?;

        let rel = node_by_dom_id(&document, &tree, "rel")?;
        let inner = node_by_dom_id(&document, &tree, "inner")?;
        let after = node_by_dom_id(&document, &tree, "after")?;
        assert_eq!(rel.rect, Rect::new(5.0, 10.0, 100.0, 20.0));
        assert_eq!(inner.rect, Rect::new(5.0, 10.0, 100.0, 5.0));
        assert_eq!(after.rect, Rect::new(0.0, 20.0, 100.0, 10.0));
        Ok(())
    }

    #[test]
    fn lays_out_grid_fixed_and_fr_columns() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="grid"><div id="a"></div><div id="b"></div><div id="c"></div></div>"#,
            r#"
                #grid {
                    display: grid;
                    grid-template-columns: 100px 1fr 1fr;
                    gap: 10px;
                    width: 400px;
                }
                #a, #b, #c { height: 20px; }
            "#,
        )?;

        let grid = node_by_dom_id(&document, &tree, "grid")?;
        let a = node_by_dom_id(&document, &tree, "a")?;
        let b = node_by_dom_id(&document, &tree, "b")?;
        let c = node_by_dom_id(&document, &tree, "c")?;

        assert_eq!(grid.rect, Rect::new(0.0, 0.0, 400.0, 20.0));
        assert_eq!(a.rect, Rect::new(0.0, 0.0, 100.0, 20.0));
        assert_eq!(b.rect, Rect::new(110.0, 0.0, 140.0, 20.0));
        assert_eq!(c.rect, Rect::new(260.0, 0.0, 140.0, 20.0));
        Ok(())
    }

    #[test]
    fn lays_out_grid_rows_and_spans() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="grid"><div id="a"></div><div id="b"></div><div id="c"></div><div id="d"></div></div>"#,
            r#"
                #grid {
                    display: grid;
                    grid-template-columns: 50px 50px 50px;
                    grid-template-rows: 30px 40px;
                    width: 150px;
                }
                #a { grid-column: span 2; height: 10px; }
                #b, #c, #d { height: 10px; }
            "#,
        )?;

        let grid = node_by_dom_id(&document, &tree, "grid")?;
        let a = node_by_dom_id(&document, &tree, "a")?;
        let b = node_by_dom_id(&document, &tree, "b")?;
        let c = node_by_dom_id(&document, &tree, "c")?;
        let d = node_by_dom_id(&document, &tree, "d")?;

        assert_eq!(grid.rect, Rect::new(0.0, 0.0, 150.0, 70.0));
        assert_eq!(a.rect, Rect::new(0.0, 0.0, 100.0, 10.0));
        assert_eq!(b.rect, Rect::new(100.0, 0.0, 50.0, 10.0));
        assert_eq!(c.rect, Rect::new(0.0, 30.0, 50.0, 10.0));
        assert_eq!(d.rect, Rect::new(50.0, 30.0, 50.0, 10.0));
        Ok(())
    }

    #[test]
    fn lays_out_grid_with_repeat_tracks_and_auto_rows() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="grid"><div id="a"></div><div id="b"></div></div>"#,
            r#"
                #grid { display: grid; grid-template-columns: repeat(2, 50px); row-gap: 10px; width: 100px; }
                #a { height: 20px; }
                #b { height: 30px; }
            "#,
        )?;

        let grid = node_by_dom_id(&document, &tree, "grid")?;
        let a = node_by_dom_id(&document, &tree, "a")?;
        let b = node_by_dom_id(&document, &tree, "b")?;

        assert_eq!(grid.rect, Rect::new(0.0, 0.0, 100.0, 30.0));
        assert_eq!(a.rect, Rect::new(0.0, 0.0, 50.0, 20.0));
        assert_eq!(b.rect, Rect::new(50.0, 0.0, 50.0, 30.0));
        Ok(())
    }

    #[test]
    fn lays_out_grid_with_explicit_placement_start() -> Result<(), TestError> {
        let (document, tree) = render(
            r#"<div id="grid"><div id="a"></div><div id="b"></div></div>"#,
            r#"
                #grid {
                    display: grid;
                    grid-template-columns: 50px 50px;
                    grid-template-rows: 20px 20px;
                }
                #a { grid-column: 2; grid-row: 1; height: 5px; }
                #b { height: 5px; }
            "#,
        )?;

        let a = node_by_dom_id(&document, &tree, "a")?;
        let b = node_by_dom_id(&document, &tree, "b")?;

assert_eq!(a.rect, Rect::new(50.0, 0.0, 50.0, 5.0));
        assert_eq!(b.rect, Rect::new(0.0, 0.0, 50.0, 5.0));
        Ok(())
    }
}
