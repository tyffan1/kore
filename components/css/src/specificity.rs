use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Specificity {
    pub ids: u16,
    pub classes: u16,
    pub elements: u16,
}

impl Specificity {
    pub const ZERO: Self = Self {
        ids: 0,
        classes: 0,
        elements: 0,
    };

    pub fn new(ids: u16, classes: u16, elements: u16) -> Self {
        Self {
            ids,
            classes,
            elements,
        }
    }

    pub fn calculate(selector: &str) -> Self {
        let mut calc = Calculator::new(selector);
        calc.calculate()
    }
}

impl Ord for Specificity {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.ids, self.classes, self.elements).cmp(&(other.ids, other.classes, other.elements))
    }
}

impl PartialOrd for Specificity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct Calculator<'a> {
    input: &'a str,
    cursor: usize,
    specificity: Specificity,
}

impl<'a> Calculator<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            cursor: 0,
            specificity: Specificity::ZERO,
        }
    }

    fn calculate(&mut self) -> Specificity {
        while let Some(ch) = self.peek() {
            match ch {
                '#' => {
                    self.bump();
                    self.read_identifier();
                    self.specificity.ids = self.specificity.ids.saturating_add(1);
                }
                '.' | '[' => {
                    self.bump();
                    self.read_selector_component(ch);
                    self.specificity.classes = self.specificity.classes.saturating_add(1);
                }
                ':' => self.pseudo(),
                '*' | '>' | '+' | '~' | ',' => {
                    self.bump();
                }
                ch if is_ident_start(ch) => {
                    self.read_identifier();
                    self.specificity.elements = self.specificity.elements.saturating_add(1);
                }
                _ => {
                    self.bump();
                }
            }
        }
        self.specificity
    }

    fn pseudo(&mut self) {
        self.bump();
        if self.peek() == Some(':') {
            self.bump();
            self.read_identifier();
            self.specificity.elements = self.specificity.elements.saturating_add(1);
            return;
        }

        let name = self.read_identifier();
        if self.peek() == Some('(') {
            let inner = self.read_parenthesized();
            if matches!(name.as_str(), "is" | "not" | "has") {
                let max_inner = inner
                    .split(',')
                    .map(Specificity::calculate)
                    .max()
                    .unwrap_or(Specificity::ZERO);
                self.specificity.ids = self.specificity.ids.saturating_add(max_inner.ids);
                self.specificity.classes =
                    self.specificity.classes.saturating_add(max_inner.classes);
                self.specificity.elements =
                    self.specificity.elements.saturating_add(max_inner.elements);
            } else if name != "where" {
                self.specificity.classes = self.specificity.classes.saturating_add(1);
            }
        } else {
            self.specificity.classes = self.specificity.classes.saturating_add(1);
        }
    }

    fn read_selector_component(&mut self, opener: char) {
        if opener == '[' {
            let mut depth = 1u16;
            while let Some(ch) = self.peek() {
                self.bump();
                match ch {
                    '[' => depth = depth.saturating_add(1),
                    ']' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        } else {
            self.read_identifier();
        }
    }

    fn read_parenthesized(&mut self) -> String {
        if self.peek() != Some('(') {
            return String::new();
        }
        self.bump();
        let start = self.cursor;
        let mut depth = 1u16;
        while let Some(ch) = self.peek() {
            self.bump();
            match ch {
                '(' => depth = depth.saturating_add(1),
                ')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        let end = self.cursor.saturating_sub(1);
                        return self.input[start..end].to_string();
                    }
                }
                _ => {}
            }
        }
        self.input[start..].to_string()
    }

    fn read_identifier(&mut self) -> String {
        let start = self.cursor;
        while let Some(ch) = self.peek() {
            if !is_ident(ch) {
                break;
            }
            self.bump();
        }
        self.input[start..self.cursor].to_ascii_lowercase()
    }

    fn peek(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
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

fn is_ident(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}
