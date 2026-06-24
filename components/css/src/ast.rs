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

// ── Animation & Transition types ──

#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub property: String,
    pub duration_ms: f32,
    pub timing: TimingFunction,
    pub delay_ms: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimingFunction {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyframeAnimation {
    pub name: String,
    pub duration_ms: f32,
    pub timing: TimingFunction,
    pub iteration_count: AnimationIterationCount,
    pub direction: AnimationDirection,
    pub fill_mode: AnimationFillMode,
    pub delay_ms: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnimationIterationCount {
    Count(f32),
    Infinite,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnimationDirection {
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnimationFillMode {
    None,
    Forwards,
    Backwards,
    Both,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransformValue {
    Translate(f32, f32),
    TranslateX(f32),
    TranslateY(f32),
    Scale(f32, f32),
    ScaleX(f32),
    ScaleY(f32),
    Rotate(f32),
    SkewX(f32),
    SkewY(f32),
    Matrix(f32, f32, f32, f32, f32, f32),
}

pub type Transform = Vec<TransformValue>;
