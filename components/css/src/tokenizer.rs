use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CssToken {
    Ident(String),
    AtKeyword(String),
    Hash(String),
    String(String),
    Number(String),
    Delim(char),
    Whitespace,
}

pub struct CssTokenizer<'a> {
    input: &'a str,
    cursor: usize,
}

impl<'a> CssTokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, cursor: 0 }
    }

    pub fn tokenize(mut self) -> Vec<CssToken> {
        let mut tokens = Vec::new();
        while let Some(ch) = self.peek() {
            match ch {
                '/' if self.peek_next() == Some('*') => self.skip_comment(),
                ch if ch.is_ascii_whitespace() => {
                    self.bump();
                    while self.peek().is_some_and(|ch| ch.is_ascii_whitespace()) {
                        self.bump();
                    }
                    tokens.push(CssToken::Whitespace);
                }
                '@' => {
                    self.bump();
                    tokens.push(CssToken::AtKeyword(self.read_identifier()));
                }
                '#' => {
                    self.bump();
                    tokens.push(CssToken::Hash(self.read_identifier()));
                }
                '"' | '\'' => tokens.push(CssToken::String(self.read_string(ch))),
                ch if ch.is_ascii_digit() => tokens.push(CssToken::Number(self.read_number())),
                ch if is_ident_start(ch) => tokens.push(CssToken::Ident(self.read_identifier())),
                _ => {
                    self.bump();
                    tokens.push(CssToken::Delim(ch));
                }
            }
        }
        tokens
    }

    pub(crate) fn strip_comments(input: &'a str) -> String {
        let mut scanner = Self::new(input);
        let mut output = String::with_capacity(input.len());
        while let Some(ch) = scanner.peek() {
            if ch == '/' && scanner.peek_next() == Some('*') {
                scanner.skip_comment();
            } else {
                output.push(ch);
                scanner.bump();
            }
        }
        output
    }

    fn skip_comment(&mut self) {
        self.bump();
        self.bump();
        while let Some(ch) = self.peek() {
            self.bump();
            if ch == '*' && self.peek() == Some('/') {
                self.bump();
                break;
            }
        }
    }

    fn read_string(&mut self, quote: char) -> String {
        self.bump();
        let start = self.cursor;
        while let Some(ch) = self.peek() {
            if ch == quote {
                let value = self.input[start..self.cursor].to_string();
                self.bump();
                return value;
            }
            self.bump();
        }
        self.input[start..].to_string()
    }

    fn read_number(&mut self) -> String {
        let start = self.cursor;
        while let Some(ch) = self.peek() {
            if !(ch.is_ascii_digit() || ch == '.') {
                break;
            }
            self.bump();
        }
        self.input[start..self.cursor].to_string()
    }

    fn read_identifier(&mut self) -> String {
        let start = self.cursor;
        while let Some(ch) = self.peek() {
            if !(is_ident_start(ch) || ch.is_ascii_digit()) {
                break;
            }
            self.bump();
        }
        self.input[start..self.cursor].to_ascii_lowercase()
    }

    fn peek(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut chars = self.input[self.cursor..].chars();
        chars.next();
        chars.next()
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
