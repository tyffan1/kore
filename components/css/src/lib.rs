//! CSS parser, specificity, and cascade foundation for Kore.

mod ast;
mod cascade;
mod color;
mod parser;
mod specificity;
mod tokenizer;

pub use ast::{
    AtRulePrelude, Declaration, KeyframeBlock, KeyframesRule, MediaRule, ParseDiagnostic, Rule,
    Selector, StyleRule, StyleSheet, SupportsRule, UnknownRule,
};
pub use cascade::{cascade_for_element, CascadedProperty, ElementSnapshot};
pub use color::CssColor;
pub use parser::{parse_stylesheet, CssParser, ParserError};
pub use specificity::Specificity;
pub use tokenizer::{CssToken, CssTokenizer};
