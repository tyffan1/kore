//! Layout tree construction and box positioning for Kore.

mod engine;
mod geometry;
mod style;
mod tree;

pub use engine::{layout_document, LayoutConfig, LayoutError};
pub use geometry::{BoxEdges, Rect};
pub use style::{AlignItems, ComputedStyle, Display, FlexDirection, FlexWrap, JustifyContent};
pub use tree::{LayoutNode, LayoutNodeId, LayoutTree};
