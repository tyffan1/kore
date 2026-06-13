use crate::{ComputedStyle, Rect};
use kore_html::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LayoutNodeId(pub usize);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutNode {
    pub id: LayoutNodeId,
    pub dom_node_id: Option<NodeId>,
    pub parent: Option<LayoutNodeId>,
    pub children: Vec<LayoutNodeId>,
    pub style: ComputedStyle,
    pub rect: Rect,
}

impl LayoutNode {
    pub fn content_rect(&self) -> Rect {
        Rect {
            x: self.rect.x + self.style.border.left + self.style.padding.left,
            y: self.rect.y + self.style.border.top + self.style.padding.top,
            width: self.style.content_width(self.rect.width),
            height: (self.rect.height
                - self.style.border.vertical()
                - self.style.padding.vertical())
            .max(0.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutTree {
    pub root: LayoutNodeId,
    pub nodes: Vec<LayoutNode>,
}

impl LayoutTree {
    pub fn node(&self, id: LayoutNodeId) -> Option<&LayoutNode> {
        self.nodes.get(id.0)
    }
}
