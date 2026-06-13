/// A simplified DOM node representation for the Elements panel.
#[derive(Debug, Clone)]
pub struct DomNode {
    pub tag_name: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<DomNode>,
    pub text_content: Option<String>,
}

impl DomNode {
    pub fn new(tag_name: impl Into<String>) -> Self {
        Self {
            tag_name: tag_name.into(),
            id: None,
            classes: Vec::new(),
            attributes: Vec::new(),
            children: Vec::new(),
            text_content: None,
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_class(mut self, class: impl Into<String>) -> Self {
        self.classes.push(class.into());
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text_content = Some(text.into());
        self
    }

    pub fn with_children(mut self, children: Vec<DomNode>) -> Self {
        self.children = children;
        self
    }

    /// Render the node and its children as an indented text tree.
    pub fn render(&self) -> String {
        let mut out = String::new();
        self.render_indent(&mut out, 0);
        out
    }

    fn render_indent(&self, out: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);
        out.push_str(&indent);
        out.push('<');
        out.push_str(&self.tag_name);

        if let Some(ref id) = self.id {
            out.push_str(&format!(" id=\"{id}\""));
        }
        if !self.classes.is_empty() {
            out.push_str(" class=\"");
            out.push_str(&self.classes.join(" "));
            out.push('"');
        }
        for (name, val) in &self.attributes {
            out.push(' ');
            out.push_str(name);
            out.push_str("=\"");
            out.push_str(val);
            out.push('"');
        }
        out.push('>');

        if let Some(ref text) = self.text_content {
            out.push_str(text);
        }

        if !self.children.is_empty() {
            out.push('\n');
            for child in &self.children {
                child.render_indent(out, depth + 1);
                out.push('\n');
            }
            out.push_str(&indent);
        }

        out.push_str("</");
        out.push_str(&self.tag_name);
        out.push('>');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_simple_element() {
        let node = DomNode::new("div");
        assert_eq!(node.render(), "<div></div>");
    }

    #[test]
    fn renders_with_id_and_class() {
        let node = DomNode::new("div").with_id("main").with_class("container");
        assert_eq!(node.render(), "<div id=\"main\" class=\"container\"></div>");
    }

    #[test]
    fn renders_with_text() {
        let node = DomNode::new("p").with_text("Hello");
        assert_eq!(node.render(), "<p>Hello</p>");
    }

    #[test]
    fn renders_nested_children() {
        let node = DomNode::new("ul").with_children(vec![
            DomNode::new("li").with_text("A"),
            DomNode::new("li").with_text("B"),
        ]);
        let rendered = node.render();
        assert!(rendered.contains("<ul>"));
        assert!(rendered.contains("<li>A</li>"));
        assert!(rendered.contains("<li>B</li>"));
        assert!(rendered.contains("</ul>"));
    }

    #[test]
    fn renders_with_attributes() {
        let node = DomNode::new("a")
            .with_children(vec![])
            .with_text("click");
        let node = DomNode {
            tag_name: "a".to_string(),
            id: None,
            classes: Vec::new(),
            attributes: vec![
                ("href".to_string(), "https://kore.dev".to_string()),
            ],
            children: Vec::new(),
            text_content: Some("click".to_string()),
        };
        let rendered = node.render();
        assert!(rendered.contains("href=\"https://kore.dev\""));
    }
}
