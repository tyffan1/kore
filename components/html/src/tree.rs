use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Element {
    pub tag_name: String,
    pub attributes: Vec<Attribute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Document,
    Doctype(String),
    Element(Element),
    Text(String),
    Comment(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    nodes: Vec<Node>,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    pub fn new() -> Self {
        let root = Node {
            id: NodeId(0),
            parent: None,
            children: Vec::new(),
            kind: NodeKind::Document,
        };
        Self { nodes: vec![root] }
    }

    pub fn root(&self) -> NodeId {
        NodeId(0)
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0)
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.0)
    }

    pub fn append(&mut self, parent: NodeId, kind: NodeKind) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            id,
            parent: Some(parent),
            children: Vec::new(),
            kind,
        });
        if let Some(parent_node) = self.nodes.get_mut(parent.0) {
            parent_node.children.push(id);
        }
        id
    }

    pub fn elements_by_tag<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.nodes.iter().filter(move |node| match &node.kind {
            NodeKind::Element(element) => element.tag_name.eq_ignore_ascii_case(tag),
            _ => false,
        })
    }
}
