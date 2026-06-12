//! Streaming-oriented HTML tokenizer and tree builder foundation.

mod parser;
mod tokenizer;
mod tree;

pub use parser::{parse_document, HtmlParser};
pub use tokenizer::{HtmlTokenizer, Token, TokenizerError};
pub use tree::{Attribute, Document, Element, Node, NodeId, NodeKind};
