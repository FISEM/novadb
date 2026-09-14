use crate::token::Token;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LexError {
    #[error("unexpected character '{0}' at position {1}")]
    UnexpectedChar(char, usize),
    #[error("unterminated string literal starting at position {0}")]
    UnterminatedString(usize),
}

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer { src: src.as_bytes(), pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.src.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(b) if b.is_ascii_whitespace() => {
                    self.pos += 1;
                }
                Some(b'-') if self.peek_at(1) == Some(b'-') => {
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    self.pos += 2;
                    while self.pos < self.src.len() {
                        if self.peek() == Some(b'*') && self.peek_at(1) == Some(b'/') {
                            self.pos += 2;
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            let start = self.pos;
            let Some(c) = self.peek() else {
                tokens.push(Token::Eof);
                break;
            };

            let tok = match c {
                b'0'..=b'9' => self.lex_number(),
                b'\'' => self.lex_string()?,
                b'"' => self.lex_quoted_ident()?,
                b'_' | b'a'..=b'z' | b'A'..=b'Z' => self.lex_ident_or_keyword(),
                b'*' => { self.bump(); Token::Star }
                b',' => { self.bump(); Token::Comma }
                b'.' => { self.bump(); Token::Dot }
                b';' => { self.bump(); Token::Semicolon }
                b'(' => { self.bump(); Token::LParen }
                b')' => { self.bump(); Token::RParen }
                b'+' => { self.bump(); Token::Plus }
                b'/' => { self.bump(); Token::Slash }
                b'%' => { self.bump(); Token::Percent }
                b'=' => { self.bump(); Token::Eq }
                b'!' => {
                    self.bump();
                    if self.peek() == Some(b'=') {
                        self.bump();
                        Token::NotEq
                    } else {
                        return Err(LexError::UnexpectedChar('!', start));
                    }
                }
                b'<' => {
                    self.bump();
                    match self.peek() {
                        Some(b'=') => { self.bump(); Token::LtEq }
                        Some(b'>') => { self.bump(); Token::NotEq }
                        _ => Token::Lt,
                    }
                }
                b'>' => {
                    self.bump();
                    if self.peek() == Some(b'=') {
                        self.bump();
                        Token::GtEq
                    } else {
                        Token::Gt
                    }
                }
                b'-' => {
                    self.bump();
                    if self.peek() == Some(b'>') {
                        self.bump();
                        if self.peek() == Some(b'>') {
                            self.bump();
                            Token::ArrowText
                        } else {
                            Token::Arrow
                        }
                    } else {
                        Token::Minus
                    }
                }
                b'|' => {
                    self.bump();
                    if self.peek() == Some(b'|') {
                        self.bump();
                        Token::Concat
                    } else {
                        return Err(LexError::UnexpectedChar('|', start));
                    }
                }
                other => return Err(LexError::UnexpectedChar(other as char, start)),
            };
            tokens.push(tok);
        }
        Ok(tokens)
    }

    fn lex_number(&mut self) -> Token {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') && matches!(self.peek_at(1), Some(b'0'..=b'9')) {
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            let save = self.pos;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            } else {
                self.pos = save;
            }
        }
        let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap().to_string();
        Token::Number(s)
    }

    fn lex_string(&mut self) -> Result<Token, LexError> {
        let start = self.pos;
        self.bump(); // consume opening quote
        let mut out = String::new();
        loop {
            match self.bump() {
                Some(b'\'') => {
                    if self.peek() == Some(b'\'') {
                        out.push('\'');
                        self.bump();
                    } else {
                        break;
                    }
                }
                Some(c) => out.push(c as char),
                None => return Err(LexError::UnterminatedString(start)),
            }
        }
        Ok(Token::String(out))
    }

    fn lex_quoted_ident(&mut self) -> Result<Token, LexError> {
        let start = self.pos;
        self.bump(); // consume opening quote
        let mut out = String::new();
        loop {
            match self.bump() {
                Some(b'"') => {
                    if self.peek() == Some(b'"') {
                        out.push('"');
                        self.bump();
                    } else {
                        break;
                    }
                }
                Some(c) => out.push(c as char),
                None => return Err(LexError::UnterminatedString(start)),
            }
        }
        Ok(Token::Ident(out))
    }

    fn lex_ident_or_keyword(&mut self) -> Token {
        let start = self.pos;
        while matches!(self.peek(), Some(b'_') | Some(b'a'..=b'z') | Some(b'A'..=b'Z') | Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.src[start..self.pos]).unwrap();
        keyword_or_ident(s)
    }
}

fn keyword_or_ident(s: &str) -> Token {
    match s.to_ascii_uppercase().as_str() {
        "SELECT" => Token::Select,
        "FROM" => Token::From,
        "WHERE" => Token::Where,
        "INSERT" => Token::Insert,
        "INTO" => Token::Into,
        "VALUES" => Token::Values,
        "UPDATE" => Token::Update,
        "SET" => Token::Set,
        "DELETE" => Token::Delete,
        "CREATE" => Token::Create,
        "TABLE" => Token::Table,
        "DROP" => Token::Drop,
        "IF" => Token::If,
        "NOT" => Token::Not,
        "EXISTS" => Token::Exists,
        "NULL" => Token::Null,
        "TRUE" => Token::True,
        "FALSE" => Token::False,
        "AND" => Token::And,
        "OR" => Token::Or,
        "IS" => Token::Is,
        "IN" => Token::In,
        "BETWEEN" => Token::Between,
        "LIKE" => Token::Like,
        "ORDER" => Token::Order,
        "BY" => Token::By,
        "ASC" => Token::Asc,
        "DESC" => Token::Desc,
        "LIMIT" => Token::Limit,
        "OFFSET" => Token::Offset,
        "GROUP" => Token::Group,
        "HAVING" => Token::Having,
        "JOIN" => Token::Join,
        "INNER" => Token::Inner,
        "LEFT" => Token::Left,
        "OUTER" => Token::Outer,
        "ON" => Token::On,
        "AS" => Token::As,
        "WITH" => Token::With,
        "RECURSIVE" => Token::Recursive,
        "UNION" => Token::Union,
        "ALL" => Token::All,
        "DISTINCT" => Token::Distinct,
        "PRIMARY" => Token::Primary,
        "KEY" => Token::Key,
        "DEFAULT" => Token::Default,
        "INTEGER" | "INT" => Token::TypeInteger,
        "BIGINT" => Token::TypeBigInt,
        "DOUBLE" | "FLOAT" | "REAL" | "NUMERIC" => Token::TypeDouble,
        "TEXT" | "VARCHAR" | "CHAR" | "STRING" => Token::TypeText,
        "BOOLEAN" | "BOOL" => Token::TypeBoolean,
        "JSON" | "JSONB" => Token::TypeJson,
        "TIMESTAMP" | "DATETIME" | "DATE" => Token::TypeTimestamp,
        _ => Token::Ident(s.to_string()),
    }
}
