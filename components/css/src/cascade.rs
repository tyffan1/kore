use crate::{Declaration, Rule, Selector, Specificity, StyleSheet};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementSnapshot {
    pub tag_name: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub attributes: Vec<String>,
}

impl ElementSnapshot {
    pub fn new(tag_name: &str) -> Self {
        Self {
            tag_name: tag_name.to_ascii_lowercase(),
            id: None,
            classes: Vec::new(),
            attributes: Vec::new(),
        }
    }

    pub fn with_id(mut self, id: &str) -> Self {
        self.id = Some(id.to_string());
        self
    }

    pub fn with_class(mut self, class: &str) -> Self {
        self.classes.push(class.to_string());
        self
    }

    pub fn with_attribute(mut self, attribute: &str) -> Self {
        self.attributes.push(attribute.to_ascii_lowercase());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CascadedProperty {
    pub property: String,
    pub value: String,
    pub specificity: Specificity,
    pub important: bool,
    pub source_order: usize,
}

#[derive(Debug, Clone)]
struct Candidate<'a> {
    declaration: &'a Declaration,
    specificity: Specificity,
}

pub fn cascade_for_element(sheet: &StyleSheet, element: &ElementSnapshot) -> Vec<CascadedProperty> {
    let mut winners: BTreeMap<String, Candidate<'_>> = BTreeMap::new();
    collect_rules(&sheet.rules, element, &mut winners);
    winners
        .into_iter()
        .map(|(property, candidate)| CascadedProperty {
            property,
            value: candidate.declaration.value.clone(),
            specificity: candidate.specificity,
            important: candidate.declaration.important,
            source_order: candidate.declaration.source_order,
        })
        .collect()
}

fn collect_rules<'a>(
    rules: &'a [Rule],
    element: &ElementSnapshot,
    winners: &mut BTreeMap<String, Candidate<'a>>,
) {
    for rule in rules {
        match rule {
            Rule::Style(style) => {
                for selector in &style.selectors {
                    if !matches_selector(selector, element) {
                        continue;
                    }
                    for declaration in &style.declarations {
                        let candidate = Candidate {
                            declaration,
                            specificity: selector.specificity,
                        };
                        apply_candidate(winners, candidate);
                    }
                }
            }
            Rule::Media(media) => collect_rules(&media.rules, element, winners),
            Rule::Supports(supports) => collect_rules(&supports.rules, element, winners),
            Rule::Keyframes(_) | Rule::Unknown(_) => {}
        }
    }
}

fn apply_candidate<'a>(winners: &mut BTreeMap<String, Candidate<'a>>, candidate: Candidate<'a>) {
    let property = candidate.declaration.property.clone();
    let should_replace = winners
        .get(&property)
        .map(|current| compare_candidate(&candidate, current).is_gt())
        .unwrap_or(true);
    if should_replace {
        winners.insert(property, candidate);
    }
}

fn compare_candidate(left: &Candidate<'_>, right: &Candidate<'_>) -> std::cmp::Ordering {
    (
        left.declaration.important,
        left.specificity,
        left.declaration.source_order,
    )
        .cmp(&(
            right.declaration.important,
            right.specificity,
            right.declaration.source_order,
        ))
}

fn matches_selector(selector: &Selector, element: &ElementSnapshot) -> bool {
    let compound = last_compound_selector(&selector.text);
    if compound.is_empty() {
        return false;
    }
    CompoundMatcher::new(compound, element).matches()
}

fn last_compound_selector(selector: &str) -> &str {
    selector
        .rsplit(|ch: char| ch.is_ascii_whitespace() || matches!(ch, '>' | '+' | '~'))
        .find(|part| !part.trim().is_empty())
        .map(str::trim)
        .unwrap_or(selector.trim())
}

struct CompoundMatcher<'a> {
    selector: &'a str,
    element: &'a ElementSnapshot,
    cursor: usize,
}

impl<'a> CompoundMatcher<'a> {
    fn new(selector: &'a str, element: &'a ElementSnapshot) -> Self {
        Self {
            selector,
            element,
            cursor: 0,
        }
    }

    fn matches(&mut self) -> bool {
        while let Some(ch) = self.peek() {
            match ch {
                '#' => {
                    self.bump();
                    let id = self.read_identifier();
                    if self.element.id.as_deref() != Some(id.as_str()) {
                        return false;
                    }
                }
                '.' => {
                    self.bump();
                    let class = self.read_identifier();
                    if !self.element.classes.iter().any(|item| item == &class) {
                        return false;
                    }
                }
                '[' => {
                    let attr = self.read_attribute_name();
                    if !self.element.attributes.iter().any(|item| item == &attr) {
                        return false;
                    }
                }
                ':' => {
                    self.skip_pseudo();
                }
                '*' => {
                    self.bump();
                }
                ch if is_ident_start(ch) => {
                    let tag = self.read_identifier();
                    if tag != self.element.tag_name {
                        return false;
                    }
                }
                _ => self.bump(),
            }
        }
        true
    }

    fn read_attribute_name(&mut self) -> String {
        self.bump();
        let start = self.cursor;
        while let Some(ch) = self.peek() {
            if matches!(ch, ']' | '=' | '~' | '|' | '^' | '$' | '*') {
                break;
            }
            self.bump();
        }
        let name = self.selector[start..self.cursor]
            .trim()
            .to_ascii_lowercase();
        while let Some(ch) = self.peek() {
            self.bump();
            if ch == ']' {
                break;
            }
        }
        name
    }

    fn skip_pseudo(&mut self) {
        self.bump();
        while let Some(ch) = self.peek() {
            if !(is_ident_start(ch) || ch.is_ascii_digit()) {
                break;
            }
            self.bump();
        }
        if self.peek() == Some('(') {
            let mut depth = 0u16;
            while let Some(ch) = self.peek() {
                self.bump();
                match ch {
                    '(' => depth = depth.saturating_add(1),
                    ')' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn read_identifier(&mut self) -> String {
        let start = self.cursor;
        while let Some(ch) = self.peek() {
            if !(is_ident_start(ch) || ch.is_ascii_digit()) {
                break;
            }
            self.bump();
        }
        self.selector[start..self.cursor].to_ascii_lowercase()
    }

    fn peek(&self) -> Option<char> {
        self.selector[self.cursor..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(ch) = self.peek() {
            self.cursor += ch.len_utf8();
        }
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || ch == '-'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_stylesheet, ParserError};

    #[test]
    fn cascade_prefers_specific_selector_then_source_order() -> Result<(), ParserError> {
        let sheet = parse_stylesheet(
            "button { color: black } .primary { color: blue } button { margin: 0 }",
        )?;
        let element = ElementSnapshot::new("button").with_class("primary");
        let cascaded = cascade_for_element(&sheet, &element);
        let color = cascaded.iter().find(|item| item.property == "color");
        let margin = cascaded.iter().find(|item| item.property == "margin");

        assert!(matches!(color, Some(item) if item.value == "blue"));
        assert!(matches!(margin, Some(item) if item.value == "0"));
        Ok(())
    }

    #[test]
    fn important_beats_higher_specificity() -> Result<(), ParserError> {
        let sheet = parse_stylesheet("#app { color: blue } button { color: red !important }")?;
        let element = ElementSnapshot::new("button").with_id("app");
        let cascaded = cascade_for_element(&sheet, &element);
        let color = cascaded.iter().find(|item| item.property == "color");

        assert!(matches!(color, Some(item) if item.value == "red" && item.important));
        Ok(())
    }

    #[test]
    fn selector_matching_supports_id_class_and_attribute() -> Result<(), ParserError> {
        let sheet = parse_stylesheet("input#search.control[type] { outline: none }")?;
        let element = ElementSnapshot::new("input")
            .with_id("search")
            .with_class("control")
            .with_attribute("type");
        let cascaded = cascade_for_element(&sheet, &element);

        assert_eq!(cascaded.len(), 1);
        assert_eq!(cascaded[0].property, "outline");
        Ok(())
    }
}
