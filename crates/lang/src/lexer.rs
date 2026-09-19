//! Turns nova source into tokens, including the indentation ones.
//!
//! Indentation is meaningful, so the lexer is where it stops being
//! whitespace and becomes structure: a line indented further than the one
//! before it opens a block, and coming back out closes it.

use serde_json::Number;

use crate::ast::Span;
use crate::ParseError;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Name(String),
    Number(Number),
    Text(String),

    Pipe,
    Comma,
    Colon,
    Equals,
    Dot,
    Question,
    Semicolon,
    OpenParen,
    CloseParen,
    OpenSquare,
    CloseSquare,
    OpenCurly,
    CloseCurly,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,

    /// End of a line that carried something.
    Newline,
    /// A line indented further than the one before it.
    Indent,
    /// A line indented less than the one before it.
    Dedent,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

pub fn tokenize(source: &str) -> Result<Vec<Token>, ParseError> {
    Lexer::new(source).run()
}

struct Lexer<'a> {
    src: &'a [u8],
    text: &'a str,
    at: usize,
    out: Vec<Token>,
    indents: Vec<usize>,
    /// Inside brackets, line breaks are just spaces.
    depth: usize,
    /// Whether the current line has produced anything yet.
    line_has_content: bool,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Self {
        Lexer {
            src: text.as_bytes(),
            text,
            at: 0,
            out: Vec::new(),
            indents: vec![0],
            depth: 0,
            line_has_content: false,
        }
    }

    fn run(mut self) -> Result<Vec<Token>, ParseError> {
        self.open_line()?;
        while self.at < self.src.len() {
            let c = self.src[self.at];
            match c {
                b' ' | b'\t' | b'\r' => self.at += 1,
                b'#' => while self.at < self.src.len() && self.src[self.at] != b'\n' {
                    self.at += 1;
                },
                b'\n' => {
                    self.at += 1;
                    if self.depth == 0 {
                        if self.line_has_content {
                            self.push(Tok::Newline, self.at - 1, self.at);
                            self.line_has_content = false;
                        }
                        self.open_line()?;
                    }
                }
                b'"' | b'\'' => self.read_text()?,
                b'0'..=b'9' => self.read_number()?,
                c if c == b'_' || c.is_ascii_alphabetic() => self.read_name(),
                _ => self.read_symbol()?,
            }
        }

        if self.line_has_content {
            self.push(Tok::Newline, self.at, self.at);
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.push(Tok::Dedent, self.at, self.at);
        }
        self.push(Tok::End, self.at, self.at);
        Ok(self.out)
    }

    /// Measures the indentation of the line starting at `at`, skipping lines
    /// that hold nothing but spaces or a comment, and emits Indent or Dedent.
    fn open_line(&mut self) -> Result<(), ParseError> {
        loop {
            let start = self.at;
            let mut width = 0;
            while self.at < self.src.len() && matches!(self.src[self.at], b' ' | b'\t') {
                width += 1;
                self.at += 1;
            }
            // A line with nothing on it carries no indentation information.
            match self.src.get(self.at) {
                None => return Ok(()),
                Some(b'\n') => {
                    self.at += 1;
                    continue;
                }
                Some(b'\r') => {
                    self.at += 1;
                    continue;
                }
                Some(b'#') => {
                    while self.at < self.src.len() && self.src[self.at] != b'\n' {
                        self.at += 1;
                    }
                    continue;
                }
                Some(_) => {}
            }

            let current = *self.indents.last().expect("one level is always there");
            if width > current {
                self.indents.push(width);
                self.push(Tok::Indent, start, self.at);
            } else if width < current {
                while *self.indents.last().expect("one level is always there") > width {
                    self.indents.pop();
                    self.push(Tok::Dedent, start, self.at);
                }
                if *self.indents.last().expect("one level is always there") != width {
                    return Err(ParseError {
                        message: "This line is indented less than the one above it, but does not line up with any step before that.".to_string(),
                        span: Span { start, end: self.at },
                        help: Some("Every step in a pipeline lines up at the same depth.".to_string()),
                    });
                }
            }
            return Ok(());
        }
    }

    fn read_name(&mut self) {
        let start = self.at;
        while self.at < self.src.len()
            && (self.src[self.at] == b'_' || self.src[self.at].is_ascii_alphanumeric())
        {
            self.at += 1;
        }
        let word = self.text[start..self.at].to_string();
        self.push(Tok::Name(word), start, self.at);
    }

    fn read_number(&mut self) -> Result<(), ParseError> {
        let start = self.at;
        while self.at < self.src.len() && self.src[self.at].is_ascii_digit() {
            self.at += 1;
        }
        let mut floating = false;
        if self.src.get(self.at) == Some(&b'.')
            && self.src.get(self.at + 1).is_some_and(|c| c.is_ascii_digit())
        {
            floating = true;
            self.at += 1;
            while self.at < self.src.len() && self.src[self.at].is_ascii_digit() {
                self.at += 1;
            }
        }
        let raw = &self.text[start..self.at];
        let number = if floating {
            raw.parse::<f64>().ok().and_then(Number::from_f64)
        } else {
            raw.parse::<i64>().ok().map(Number::from)
        };
        match number {
            Some(n) => {
                self.push(Tok::Number(n), start, self.at);
                Ok(())
            }
            None => Err(ParseError {
                message: format!("'{raw}' is too large to be a number."),
                span: Span { start, end: self.at },
                help: None,
            }),
        }
    }

    fn read_text(&mut self) -> Result<(), ParseError> {
        let start = self.at;
        let quote = self.src[self.at];
        self.at += 1;
        let mut value = String::new();
        loop {
            match self.src.get(self.at) {
                None | Some(b'\n') => {
                    return Err(ParseError {
                        message: "This text never ends.".to_string(),
                        span: Span { start, end: self.at },
                        help: Some(format!("Add a closing {} quote.", quote as char)),
                    })
                }
                Some(&c) if c == quote => {
                    self.at += 1;
                    self.push(Tok::Text(value), start, self.at);
                    return Ok(());
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escaped = match self.src.get(self.at) {
                        Some(b'n') => '\n',
                        Some(b't') => '\t',
                        Some(&c) => c as char,
                        None => continue,
                    };
                    value.push(escaped);
                    self.at += 1;
                }
                Some(&c) => {
                    // Walk by character, not by byte, so text stays text.
                    let rest = &self.text[self.at..];
                    let ch = rest.chars().next().unwrap_or(c as char);
                    value.push(ch);
                    self.at += ch.len_utf8();
                }
            }
        }
    }

    fn read_symbol(&mut self) -> Result<(), ParseError> {
        let start = self.at;
        let two = self.text.get(start..start + 2);
        let (tok, width) = match two {
            Some("==") => (Tok::EqualEqual, 2),
            Some("!=") => (Tok::NotEqual, 2),
            Some("<=") => (Tok::LessEqual, 2),
            Some(">=") => (Tok::GreaterEqual, 2),
            _ => {
                let one = match self.src[start] {
                    b'|' => Tok::Pipe,
                    b',' => Tok::Comma,
                    b':' => Tok::Colon,
                    b'=' => Tok::Equals,
                    b'.' => Tok::Dot,
                    b'?' => Tok::Question,
                    b';' => Tok::Semicolon,
                    b'(' => Tok::OpenParen,
                    b')' => Tok::CloseParen,
                    b'[' => Tok::OpenSquare,
                    b']' => Tok::CloseSquare,
                    b'{' => Tok::OpenCurly,
                    b'}' => Tok::CloseCurly,
                    b'+' => Tok::Plus,
                    b'-' => Tok::Minus,
                    b'*' => Tok::Star,
                    b'/' => Tok::Slash,
                    b'%' => Tok::Percent,
                    b'<' => Tok::Less,
                    b'>' => Tok::Greater,
                    _ => {
                        let ch = self.text[start..].chars().next().unwrap_or('?');
                        return Err(ParseError {
                            message: format!("'{ch}' does not mean anything here."),
                            span: Span { start, end: start + ch.len_utf8() },
                            help: None,
                        });
                    }
                };
                (one, 1)
            }
        };
        match tok {
            Tok::OpenParen | Tok::OpenSquare | Tok::OpenCurly => self.depth += 1,
            Tok::CloseParen | Tok::CloseSquare | Tok::CloseCurly => {
                self.depth = self.depth.saturating_sub(1)
            }
            _ => {}
        }
        self.at = start + width;
        self.push(tok, start, self.at);
        Ok(())
    }

    fn push(&mut self, tok: Tok, start: usize, end: usize) {
        if !matches!(tok, Tok::Indent | Tok::Dedent | Tok::Newline | Tok::End) {
            self.line_has_content = true;
        }
        self.out.push(Token { tok, span: Span { start, end } });
    }
}
