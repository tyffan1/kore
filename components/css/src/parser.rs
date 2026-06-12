use crate::{
    AtRulePrelude, Declaration, KeyframeBlock, KeyframesRule, MediaRule, ParseDiagnostic, Rule,
    Selector, Specificity, StyleRule, StyleSheet, SupportsRule, UnknownRule,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParserError {
    #[error("parser failed to make progress at byte {0}")]
    Stalled(usize),
}

#[derive(Debug)]
pub struct CssParser {
    input: String,
    cursor: usize,
    diagnostics: Vec<ParseDiagnostic>,
    source_order: usize,
}

impl CssParser {
    pub fn new(input: &str) -> Self {
        Self {
            input: crate::CssTokenizer::strip_comments(input),
            cursor: 0,
            diagnostics: Vec::new(),
            source_order: 0,
        }
    }

    pub fn parse(mut self) -> Result<StyleSheet, ParserError> {
        let rules = self.parse_rule_list(None)?;
        Ok(StyleSheet::new(rules, self.diagnostics))
    }

    fn parse_rule_list(&mut self, terminator: Option<char>) -> Result<Vec<Rule>, ParserError> {
        let mut rules = Vec::new();
        while self.cursor < self.input.len() {
            let before = self.cursor;
            self.skip_ws();
            if terminator.is_some_and(|term| self.peek() == Some(term)) {
                self.bump();
                break;
            }
            if self.cursor >= self.input.len() {
                break;
            }

            let rule = if self.peek() == Some('@') {
                self.parse_at_rule()?
            } else {
                self.parse_style_rule()
            };
            if let Some(rule) = rule {
                rules.push(rule);
            }
            if self.cursor == before {
                return Err(ParserError::Stalled(self.cursor));
            }
        }
        Ok(rules)
    }

    fn parse_at_rule(&mut self) -> Result<Option<Rule>, ParserError> {
        let start = self.cursor;
        self.bump();
        let name = self.read_identifier();
        self.skip_ws();

        match name.as_str() {
            "media" => self.parse_nested_at_rule(start, name, AtRuleKind::Media),
            "supports" => self.parse_nested_at_rule(start, name, AtRuleKind::Supports),
            "keyframes" | "-webkit-keyframes" => Ok(self.parse_keyframes(start)),
            _ => Ok(self.parse_unknown_at_rule(start, name)),
        }
    }

    fn parse_nested_at_rule(
        &mut self,
        start: usize,
        name: String,
        kind: AtRuleKind,
    ) -> Result<Option<Rule>, ParserError> {
        let prelude = self.read_until_top_level('{').trim().to_string();
        if !self.consume('{') {
            self.diagnostic(start, format!("ignored @{name} rule without a block"));
            self.recover_to_rule_boundary();
            return Ok(None);
        }
        let order = self.next_order();
        let rules = self.parse_rule_list(Some('}'))?;
        let prelude = AtRulePrelude { text: prelude };
        let rule = match kind {
            AtRuleKind::Media => Rule::Media(MediaRule {
                prelude,
                rules,
                source_order: order,
            }),
            AtRuleKind::Supports => Rule::Supports(SupportsRule {
                prelude,
                rules,
                source_order: order,
            }),
        };
        Ok(Some(rule))
    }

    fn parse_keyframes(&mut self, start: usize) -> Option<Rule> {
        let name = self.read_until_top_level('{').trim().to_string();
        if name.is_empty() || !self.consume('{') {
            self.diagnostic(start, "ignored @keyframes rule without a name or block");
            self.recover_to_rule_boundary();
            return None;
        }

        let order = self.next_order();
        let mut frames = Vec::new();
        while self.cursor < self.input.len() {
            self.skip_ws();
            if self.consume('}') {
                break;
            }
            let selector = self.read_until_top_level('{').trim().to_ascii_lowercase();
            if selector.is_empty() || !self.consume('{') {
                self.diagnostic(self.cursor, "ignored malformed keyframe block");
                self.recover_to_rule_boundary();
                continue;
            }
            let declarations = self.parse_declarations();
            frames.push(KeyframeBlock {
                selector,
                declarations,
            });
        }

        Some(Rule::Keyframes(KeyframesRule {
            name,
            frames,
            source_order: order,
        }))
    }

    fn parse_unknown_at_rule(&mut self, start: usize, name: String) -> Option<Rule> {
        let prelude = self.read_until_rule_boundary().trim().to_string();
        let order = self.next_order();
        self.diagnostic(start, format!("skipped unknown @{name} rule"));
        if self.peek() == Some('{') {
            self.skip_balanced_block();
        } else {
            self.consume(';');
        }
        Some(Rule::Unknown(UnknownRule {
            name,
            prelude,
            source_order: order,
        }))
    }

    fn parse_style_rule(&mut self) -> Option<Rule> {
        let start = self.cursor;
        let selector_text = self.read_until_top_level('{');
        if !self.consume('{') {
            self.diagnostic(start, "ignored style rule without a declaration block");
            self.recover_to_rule_boundary();
            return None;
        }

        let selectors = selector_text
            .split(',')
            .map(str::trim)
            .filter(|selector| !selector.is_empty())
            .map(|text| Selector {
                text: text.to_string(),
                specificity: Specificity::calculate(text),
            })
            .collect::<Vec<_>>();

        if selectors.is_empty() {
            self.diagnostic(start, "ignored style rule without selectors");
            self.skip_balanced_block_body();
            return None;
        }

        let order = self.next_order();
        Some(Rule::Style(StyleRule {
            selectors,
            declarations: self.parse_declarations(),
            source_order: order,
        }))
    }

    fn parse_declarations(&mut self) -> Vec<Declaration> {
        let mut declarations = Vec::new();
        loop {
            self.skip_ws();
            if self.cursor >= self.input.len() || self.consume('}') {
                break;
            }
            let start = self.cursor;
            let property = self
                .read_until_declaration_separator()
                .trim()
                .to_ascii_lowercase();
            if property.is_empty() || !self.consume(':') {
                self.diagnostic(start, "skipped malformed declaration");
                self.recover_declaration();
                continue;
            }
            let mut value = self
                .read_until_top_level_any(&[';', '}'])
                .trim()
                .to_string();
            let important = strip_important(&mut value);
            declarations.push(Declaration {
                property,
                value,
                important,
                source_order: self.next_order(),
            });
            if self.peek() == Some(';') {
                self.bump();
            } else if self.peek() == Some('}') {
                self.bump();
                break;
            }
        }
        declarations
    }

    fn read_until_declaration_separator(&mut self) -> String {
        let start = self.cursor;
        while let Some(ch) = self.peek() {
            if matches!(ch, ':' | ';' | '}') {
                break;
            }
            self.bump();
        }
        self.input[start..self.cursor].to_string()
    }

    fn read_until_rule_boundary(&mut self) -> String {
        self.read_until_top_level_any(&[';', '{'])
    }

    fn read_until_top_level(&mut self, target: char) -> String {
        self.read_until_top_level_any(&[target])
    }

    fn read_until_top_level_any(&mut self, targets: &[char]) -> String {
        let start = self.cursor;
        let mut paren = 0u16;
        let mut bracket = 0u16;
        while let Some(ch) = self.peek() {
            if paren == 0 && bracket == 0 && targets.contains(&ch) {
                break;
            }
            match ch {
                '"' | '\'' => self.skip_string(ch),
                '(' => {
                    paren = paren.saturating_add(1);
                    self.bump();
                }
                ')' => {
                    paren = paren.saturating_sub(1);
                    self.bump();
                }
                '[' => {
                    bracket = bracket.saturating_add(1);
                    self.bump();
                }
                ']' => {
                    bracket = bracket.saturating_sub(1);
                    self.bump();
                }
                _ => self.bump(),
            }
        }
        self.input[start..self.cursor].to_string()
    }

    fn skip_balanced_block(&mut self) {
        if !self.consume('{') {
            return;
        }
        self.skip_balanced_block_body();
    }

    fn skip_balanced_block_body(&mut self) {
        let mut depth = 1u16;
        while let Some(ch) = self.peek() {
            self.bump();
            match ch {
                '"' | '\'' => self.skip_string(ch),
                '{' => depth = depth.saturating_add(1),
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    fn recover_to_rule_boundary(&mut self) {
        while let Some(ch) = self.peek() {
            self.bump();
            if matches!(ch, ';' | '}') {
                break;
            }
        }
    }

    fn recover_declaration(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == ';' {
                self.bump();
                break;
            }
            if ch == '}' {
                break;
            }
            self.bump();
        }
    }

    fn skip_string(&mut self, quote: char) {
        self.bump();
        while let Some(ch) = self.peek() {
            self.bump();
            if ch == quote {
                break;
            }
        }
    }

    fn read_identifier(&mut self) -> String {
        let start = self.cursor;
        while let Some(ch) = self.peek() {
            if !(ch.is_ascii_alphabetic() || ch.is_ascii_digit() || ch == '-' || ch == '_') {
                break;
            }
            self.bump();
        }
        self.input[start..self.cursor].to_ascii_lowercase()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|ch| ch.is_ascii_whitespace()) {
            self.bump();
        }
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(ch) = self.peek() {
            self.cursor += ch.len_utf8();
        }
    }

    fn next_order(&mut self) -> usize {
        let order = self.source_order;
        self.source_order = self.source_order.saturating_add(1);
        order
    }

    fn diagnostic(&mut self, position: usize, message: impl Into<String>) {
        self.diagnostics.push(ParseDiagnostic {
            message: message.into(),
            position,
        });
    }
}

#[derive(Debug, Clone, Copy)]
enum AtRuleKind {
    Media,
    Supports,
}

fn strip_important(value: &mut String) -> bool {
    let trimmed = value.trim_end();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.ends_with("!important") {
        *value = trimmed.to_string();
        return false;
    }

    let keep = trimmed.len().saturating_sub("!important".len());
    *value = trimmed[..keep].trim_end().to_string();
    true
}

pub fn parse_stylesheet(input: &str) -> Result<StyleSheet, ParserError> {
    CssParser::new(input).parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_selectors_properties_and_specificity() -> Result<(), ParserError> {
        let sheet = parse_stylesheet(
            "main#app.card[data-state=open] > h1.title { color: red; margin: 0 }",
        )?;
        let Rule::Style(rule) = &sheet.rules[0] else {
            return Err(ParserError::Stalled(0));
        };
        assert_eq!(rule.selectors.len(), 1);
        assert_eq!(rule.selectors[0].specificity, Specificity::new(1, 3, 2));
        assert_eq!(rule.declarations[0].property, "color");
        assert_eq!(rule.declarations[0].value, "red");
        Ok(())
    }

    #[test]
    fn parses_media_and_supports_nested_rules() -> Result<(), ParserError> {
        let sheet = parse_stylesheet(
            "@media (min-width: 40rem) { .shell { display: grid } } \
             @supports (display: grid) { main { gap: 1rem } }",
        )?;
        assert!(matches!(&sheet.rules[0], Rule::Media(rule) if rule.rules.len() == 1));
        assert!(matches!(&sheet.rules[1], Rule::Supports(rule) if rule.rules.len() == 1));
        Ok(())
    }

    #[test]
    fn parses_keyframes() -> Result<(), ParserError> {
        let sheet =
            parse_stylesheet("@keyframes fade { from { opacity: 0 } 100% { opacity: 1 } }")?;
        let Rule::Keyframes(rule) = &sheet.rules[0] else {
            return Err(ParserError::Stalled(0));
        };
        assert_eq!(rule.name, "fade");
        assert_eq!(rule.frames.len(), 2);
        assert_eq!(rule.frames[0].selector, "from");
        assert_eq!(rule.frames[1].declarations[0].value, "1");
        Ok(())
    }

    #[test]
    fn recovers_from_unknown_at_rules() -> Result<(), ParserError> {
        let sheet = parse_stylesheet(
            "@layer reset { * { box-sizing: border-box } } body { color: black }",
        )?;
        assert!(matches!(&sheet.rules[0], Rule::Unknown(rule) if rule.name == "layer"));
        assert!(matches!(&sheet.rules[1], Rule::Style(_)));
        assert_eq!(sheet.diagnostics.len(), 1);
        Ok(())
    }

    #[test]
    fn detects_important_declarations() -> Result<(), ParserError> {
        let sheet = parse_stylesheet(".alert { color: red !important; color: blue }")?;
        let Rule::Style(rule) = &sheet.rules[0] else {
            return Err(ParserError::Stalled(0));
        };
        assert!(rule.declarations[0].important);
        assert_eq!(rule.declarations[0].value, "red");
        assert!(!rule.declarations[1].important);
        Ok(())
    }
}
