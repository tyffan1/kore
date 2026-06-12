use crate::Specificity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleSheet {
    pub rules: Vec<Rule>,
    pub diagnostics: Vec<ParseDiagnostic>,
}

impl StyleSheet {
    pub fn new(rules: Vec<Rule>, diagnostics: Vec<ParseDiagnostic>) -> Self {
        Self { rules, diagnostics }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rule {
    Style(StyleRule),
    Media(MediaRule),
    Supports(SupportsRule),
    Keyframes(KeyframesRule),
    Unknown(UnknownRule),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleRule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selector {
    pub text: String,
    pub specificity: Specificity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Declaration {
    pub property: String,
    pub value: String,
    pub important: bool,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AtRulePrelude {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRule {
    pub prelude: AtRulePrelude,
    pub rules: Vec<Rule>,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportsRule {
    pub prelude: AtRulePrelude,
    pub rules: Vec<Rule>,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyframesRule {
    pub name: String,
    pub frames: Vec<KeyframeBlock>,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyframeBlock {
    pub selector: String,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnknownRule {
    pub name: String,
    pub prelude: String,
    pub source_order: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseDiagnostic {
    pub message: String,
    pub position: usize,
}
