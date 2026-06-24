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
pub use ast::{
    AnimationDirection, AnimationFillMode, AnimationIterationCount, KeyframeAnimation,
    TimingFunction, Transform, TransformValue, Transition,
};
pub use cascade::{cascade_for_element, CascadedProperty, ElementSnapshot};
pub use color::CssColor;
pub use parser::{
    parse_stylesheet, parse_time_ms, parse_timing_function, parse_transform, parse_transition,
    CssParser, ParserError,
};
pub use specificity::Specificity;
pub use tokenizer::{CssToken, CssTokenizer};
