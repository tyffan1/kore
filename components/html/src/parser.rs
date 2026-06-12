use crate::tokenizer::{HtmlTokenizer, Token, TokenizerError};
use crate::tree::{Document, Element, NodeId, NodeKind};

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

#[derive(Debug, Default)]
pub struct HtmlParser {
    document: Document,
    open_elements: Vec<NodeId>,
}

impl HtmlParser {
    pub fn new() -> Self {
        let document = Document::new();
        let open_elements = vec![document.root()];
        Self {
            document,
            open_elements,
        }
    }

    pub fn parse(input: &str) -> Result<Document, TokenizerError> {
        let tokens = HtmlTokenizer::new(input).tokenize()?;
        let mut parser = Self::new();
        for token in tokens {
            parser.push_token(token);
        }
        Ok(parser.finish())
    }

    pub fn push_token(&mut self, token: Token) {
        match token {
            Token::Doctype(name) => {
                self.document
                    .append(self.current(), NodeKind::Doctype(name));
            }
            Token::StartTag {
                name,
                attributes,
                self_closing,
            } => self.start_tag(name, attributes, self_closing),
            Token::EndTag(name) => self.end_tag(&name),
            Token::Text(text) => {
                if !text.is_empty() {
                    self.document.append(self.current(), NodeKind::Text(text));
                }
            }
            Token::Comment(comment) => {
                self.document
                    .append(self.current(), NodeKind::Comment(comment));
            }
        }
    }

    pub fn finish(self) -> Document {
        self.document
    }

    fn start_tag(
        &mut self,
        name: String,
        attributes: Vec<crate::tree::Attribute>,
        self_closing: bool,
    ) {
        self.apply_implied_end_tags(&name);
        let element = Element {
            tag_name: name.clone(),
            attributes,
        };
        let id = self
            .document
            .append(self.current(), NodeKind::Element(element));
        if !self_closing && !VOID_ELEMENTS.contains(&name.as_str()) {
            self.open_elements.push(id);
        }
    }

    fn end_tag(&mut self, name: &str) {
        while self.open_elements.len() > 1 {
            let Some(node_id) = self.open_elements.pop() else {
                return;
            };
            let matches = self
                .document
                .node(node_id)
                .and_then(|node| match &node.kind {
                    NodeKind::Element(element) => Some(element.tag_name.eq_ignore_ascii_case(name)),
                    _ => None,
                })
                .unwrap_or(false);
            if matches {
                break;
            }
        }
    }

    fn apply_implied_end_tags(&mut self, next_tag: &str) {
        let close_current = matches!(next_tag, "li" | "p");
        if !close_current {
            return;
        }
        let Some(current_id) = self.open_elements.last().copied() else {
            return;
        };
        let Some(current_node) = self.document.node(current_id) else {
            return;
        };
        let NodeKind::Element(element) = &current_node.kind else {
            return;
        };
        if element.tag_name == next_tag {
            self.open_elements.pop();
        }
    }

    fn current(&self) -> NodeId {
        self.open_elements
            .last()
            .copied()
            .unwrap_or_else(|| self.document.root())
    }
}

pub fn parse_document(input: &str) -> Result<Document, TokenizerError> {
    HtmlParser::parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeKind, Token};

    #[test]
    fn tokenizes_tags_attributes_and_text() {
        let tokens = HtmlTokenizer::new(r#"<main id="app" hidden>Hello<br/>world</main>"#)
            .tokenize()
            .unwrap();

        assert_eq!(
            tokens[0],
            Token::StartTag {
                name: "main".to_string(),
                attributes: vec![
                    crate::Attribute {
                        name: "id".to_string(),
                        value: "app".to_string()
                    },
                    crate::Attribute {
                        name: "hidden".to_string(),
                        value: String::new()
                    }
                ],
                self_closing: false
            }
        );
        assert_eq!(tokens[1], Token::Text("Hello".to_string()));
        assert_eq!(
            tokens[2],
            Token::StartTag {
                name: "br".to_string(),
                attributes: Vec::new(),
                self_closing: true
            }
        );
    }

    #[test]
    fn parses_nested_document_tree() {
        let document =
            parse_document("<!doctype html><html><body><h1>Kore</h1></body></html>").unwrap();
        assert_eq!(document.elements_by_tag("html").count(), 1);
        assert_eq!(document.elements_by_tag("body").count(), 1);
        assert_eq!(document.elements_by_tag("h1").count(), 1);
        assert!(document
            .nodes()
            .iter()
            .any(|node| node.kind == NodeKind::Text("Kore".to_string())));
    }

    #[test]
    fn keeps_void_elements_leafy() {
        let document = parse_document("<body><img src=a><p>caption</p></body>").unwrap();
        let image = document.elements_by_tag("img").next().unwrap();
        assert!(image.children.is_empty());
    }

    #[test]
    fn applies_basic_implied_end_tags() {
        let document = parse_document("<ul><li>one<li>two</ul>").unwrap();
        let items: Vec<_> = document.elements_by_tag("li").collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].children.len(), 1);
        assert_eq!(items[1].children.len(), 1);
    }
}
