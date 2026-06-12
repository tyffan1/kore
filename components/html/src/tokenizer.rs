use crate::tree::Attribute;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Token {
    Doctype(String),
    StartTag {
        name: String,
        attributes: Vec<Attribute>,
        self_closing: bool,
    },
    EndTag(String),
    Text(String),
    Comment(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TokenizerError {
    #[error("unterminated markup declaration")]
    UnterminatedDeclaration,
    #[error("unterminated quoted attribute value")]
    UnterminatedAttributeValue,
}

#[derive(Debug)]
pub struct HtmlTokenizer<'a> {
    input: &'a str,
    cursor: usize,
}

impl<'a> HtmlTokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, cursor: 0 }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, TokenizerError> {
        let mut tokens = Vec::new();
        while self.cursor < self.input.len() {
            tokens.push(self.next_token()?);
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Token, TokenizerError> {
        if self.remaining().starts_with("<!--") {
            return self.comment();
        }
        if self.remaining().starts_with("<!") {
            return self.declaration();
        }
        if self.remaining().starts_with("</") {
            return Ok(self.end_tag());
        }
        if self.remaining().starts_with('<') {
            return self.start_tag();
        }
        Ok(self.text())
    }

    fn comment(&mut self) -> Result<Token, TokenizerError> {
        self.cursor += 4;
        let Some(end) = self.remaining().find("-->") else {
            return Err(TokenizerError::UnterminatedDeclaration);
        };
        let comment = self.remaining()[..end].to_string();
        self.cursor += end + 3;
        Ok(Token::Comment(comment))
    }

    fn declaration(&mut self) -> Result<Token, TokenizerError> {
        let Some(end) = self.remaining().find('>') else {
            return Err(TokenizerError::UnterminatedDeclaration);
        };
        let declaration = self.remaining()[2..end].trim().to_string();
        self.cursor += end + 1;
        let lower = declaration.to_ascii_lowercase();
        if let Some(name) = lower.strip_prefix("doctype") {
            let original_name = declaration[7..].trim();
            let value = if original_name.is_empty() {
                name.trim().to_string()
            } else {
                original_name.to_string()
            };
            Ok(Token::Doctype(value))
        } else {
            Ok(Token::Comment(declaration))
        }
    }

    fn end_tag(&mut self) -> Token {
        self.cursor += 2;
        let end = self.remaining().find('>').unwrap_or(self.remaining().len());
        let name = self.remaining()[..end].trim().to_ascii_lowercase();
        self.cursor += end.saturating_add(1).min(self.remaining().len() + 1);
        Token::EndTag(name)
    }

    fn start_tag(&mut self) -> Result<Token, TokenizerError> {
        self.cursor += 1;
        let end = self.remaining().find('>').unwrap_or(self.remaining().len());
        let mut body = self.remaining()[..end].trim().to_string();
        let self_closing = body.ends_with('/');
        if self_closing {
            body.pop();
        }
        self.cursor += end.saturating_add(1).min(self.remaining().len() + 1);

        let mut scanner = AttributeScanner::new(body.trim());
        let name = scanner.read_name().to_ascii_lowercase();
        let attributes = scanner.read_attributes()?;
        Ok(Token::StartTag {
            name,
            attributes,
            self_closing,
        })
    }

    fn text(&mut self) -> Token {
        let end = self.remaining().find('<').unwrap_or(self.remaining().len());
        let text = self.remaining()[..end].to_string();
        self.cursor += end;
        Token::Text(text)
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.cursor..]
    }
}

struct AttributeScanner<'a> {
    input: &'a str,
    cursor: usize,
}

impl<'a> AttributeScanner<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, cursor: 0 }
    }

    fn read_name(&mut self) -> String {
        self.skip_ws();
        let start = self.cursor;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_whitespace() || ch == '/' || ch == '>' {
                break;
            }
            self.cursor += ch.len_utf8();
        }
        self.input[start..self.cursor].to_string()
    }

    fn read_attributes(&mut self) -> Result<Vec<Attribute>, TokenizerError> {
        let mut attrs = Vec::new();
        loop {
            self.skip_ws();
            if self.cursor >= self.input.len() {
                break;
            }
            let name = self.read_attr_name().to_ascii_lowercase();
            if name.is_empty() {
                break;
            }
            self.skip_ws();
            let value = if self.consume('=') {
                self.skip_ws();
                self.read_attr_value()?
            } else {
                String::new()
            };
            attrs.push(Attribute { name, value });
        }
        Ok(attrs)
    }

    fn read_attr_name(&mut self) -> String {
        let start = self.cursor;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_whitespace() || ch == '=' || ch == '/' || ch == '>' {
                break;
            }
            self.cursor += ch.len_utf8();
        }
        self.input[start..self.cursor].to_string()
    }

    fn read_attr_value(&mut self) -> Result<String, TokenizerError> {
        match self.peek() {
            Some('"') | Some('\'') => {
                let quote = self.peek().unwrap_or('"');
                self.cursor += quote.len_utf8();
                let start = self.cursor;
                while let Some(ch) = self.peek() {
                    if ch == quote {
                        let value = self.input[start..self.cursor].to_string();
                        self.cursor += quote.len_utf8();
                        return Ok(value);
                    }
                    self.cursor += ch.len_utf8();
                }
                Err(TokenizerError::UnterminatedAttributeValue)
            }
            Some(_) => {
                let start = self.cursor;
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_whitespace() || ch == '/' || ch == '>' {
                        break;
                    }
                    self.cursor += ch.len_utf8();
                }
                Ok(self.input[start..self.cursor].to_string())
            }
            None => Ok(String::new()),
        }
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.cursor += expected.len_utf8();
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek() {
            if !ch.is_ascii_whitespace() {
                break;
            }
            self.cursor += ch.len_utf8();
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
    }
}
