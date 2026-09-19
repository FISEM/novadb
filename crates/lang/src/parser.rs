//! Reads tokens into statements.

use crate::ast::*;
use crate::lexer::{Tok, Token};
use crate::ParseError;

/// Words that begin a step. Any other bare name in step position is a piece
/// defined with `define … as`.
const STEPS: &[&str] = &[
    "where", "show", "sort", "take", "skip", "unique", "join", "group", "follow", "keep", "set",
    "delete",
];

pub fn parse_tokens(tokens: Vec<Token>) -> Result<Vec<Statement>, ParseError> {
    Parser { toks: tokens, at: 0 }.program()
}

struct Parser {
    toks: Vec<Token>,
    at: usize,
}

impl Parser {
    // --- Looking around -----------------------------------------------------

    fn peek(&self) -> &Tok {
        &self.toks[self.at.min(self.toks.len() - 1)].tok
    }

    fn peek_at(&self, ahead: usize) -> &Tok {
        &self.toks[(self.at + ahead).min(self.toks.len() - 1)].tok
    }

    fn span(&self) -> Span {
        self.toks[self.at.min(self.toks.len() - 1)].span
    }

    fn advance(&mut self) -> Token {
        let t = self.toks[self.at.min(self.toks.len() - 1)].clone();
        if self.at < self.toks.len() - 1 {
            self.at += 1;
        }
        t
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == tok {
            self.advance();
            true
        } else {
            false
        }
    }

    fn at_word(&self, word: &str) -> bool {
        matches!(self.peek(), Tok::Name(w) if w == word)
    }

    fn word_at(&self, ahead: usize, word: &str) -> bool {
        matches!(self.peek_at(ahead), Tok::Name(w) if w == word)
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if self.at_word(word) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn fail<T>(&self, message: impl Into<String>, help: Option<&str>) -> Result<T, ParseError> {
        Err(ParseError {
            message: message.into(),
            span: self.span(),
            help: help.map(str::to_string),
        })
    }

    fn name(&mut self, what: &str, help: &str) -> Result<(String, Span), ParseError> {
        match self.peek().clone() {
            Tok::Name(w) => {
                let span = self.span();
                self.advance();
                Ok((w, span))
            }
            _ => self.fail(format!("{what} needs a name here."), Some(help)),
        }
    }

    fn expect(&mut self, tok: Tok, message: &str, help: &str) -> Result<(), ParseError> {
        if self.eat(&tok) {
            Ok(())
        } else {
            self.fail(message.to_string(), Some(help))
        }
    }

    // --- Program ------------------------------------------------------------

    fn program(&mut self) -> Result<Vec<Statement>, ParseError> {
        let mut out = Vec::new();
        loop {
            while matches!(
                self.peek(),
                Tok::Newline | Tok::Semicolon | Tok::Indent | Tok::Dedent
            ) {
                self.advance();
            }
            if *self.peek() == Tok::End {
                return Ok(out);
            }
            out.push(self.statement()?);
            // A block closing ends the statement that opened it: the newline
            // before the Dedent is gone by then, and demanding another one
            // would refuse two indented statements in a row.
            if self.just_closed_a_block() {
                continue;
            }
            match self.peek() {
                Tok::Newline | Tok::Semicolon | Tok::Dedent | Tok::End => {}
                _ => {
                    return self.fail(
                        "This is where one statement ends, but there is more on the line.",
                        Some("Statements are separated by a new line or a ';'."),
                    )
                }
            }
        }
    }

    /// Whether the token just consumed was the end of an indented block.
    fn just_closed_a_block(&self) -> bool {
        self.at > 0 && self.toks[self.at - 1].tok == Tok::Dedent
    }

    fn statement(&mut self) -> Result<Statement, ParseError> {
        if self.at_word("define") {
            self.define()
        } else if self.at_word("add") {
            self.add()
        } else if self.at_word("remove") {
            self.remove()
        } else {
            Ok(Statement::Query(self.pipeline(false)?))
        }
    }

    // --- define -------------------------------------------------------------

    fn define(&mut self) -> Result<Statement, ParseError> {
        let start = self.span();
        self.advance(); // define
        let (name, _) = self.name("define", "Write the name of a collection, like 'define person'.")?;

        if self.eat_word("as") {
            let pipeline = self.indented_or_inline(|p, in_block| p.pipeline(in_block))?;
            return Ok(Statement::DefineName(NameDef { name, pipeline, span: start }));
        }

        let fields = if *self.peek() == Tok::OpenCurly {
            Some(self.braced_fields()?)
        } else if *self.peek() == Tok::Newline && *self.peek_at(1) == Tok::Indent {
            self.advance();
            self.advance();
            let mut fields = Vec::new();
            loop {
                match self.peek() {
                    Tok::Dedent => {
                        self.advance();
                        break;
                    }
                    Tok::End => break,
                    Tok::Newline => {
                        self.advance();
                    }
                    _ => fields.push(self.field()?),
                }
            }
            Some(fields)
        } else {
            None
        };

        Ok(Statement::DefineShape(ShapeDef { name, fields, span: start }))
    }

    fn braced_fields(&mut self) -> Result<Vec<FieldDef>, ParseError> {
        self.advance(); // {
        let mut fields = Vec::new();
        if *self.peek() != Tok::CloseCurly {
            loop {
                fields.push(self.field()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(
            Tok::CloseCurly,
            "This shape is missing its closing '}'.",
            "Add a '}' after the last field.",
        )?;
        Ok(fields)
    }

    fn field(&mut self) -> Result<FieldDef, ParseError> {
        let (name, span) = self.name("A field", "Write a field name, like 'id: number'.")?;
        self.expect(
            Tok::Colon,
            format!("'{name}' needs a type after it.").as_str(),
            "Write ': number', ': string', ': boolean' or ': any'.",
        )?;
        let (word, type_span) = self.name("A type", "Write 'number', 'string', 'boolean' or 'any'.")?;
        let kind = match word.as_str() {
            "number" => TypeName::Number,
            "string" => TypeName::String,
            "boolean" => TypeName::Boolean,
            "any" => TypeName::Any,
            other => {
                return Err(ParseError {
                    message: format!("'{other}' is not a type."),
                    span: type_span,
                    help: Some("The types are 'number', 'string', 'boolean' and 'any'.".to_string()),
                })
            }
        };

        let mut optional = false;
        let mut key = false;
        loop {
            if self.eat(&Tok::Question) {
                optional = true;
            } else if self.eat_word("key") {
                key = true;
            } else {
                break;
            }
        }
        Ok(FieldDef { name, kind, optional, key, span })
    }

    // --- add, remove --------------------------------------------------------

    fn add(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // add
        let (name, span) = self.name("add", "Write the collection to add to, like 'add person'.")?;
        let record = self.indented_or_inline(|p, _| p.record_body())?;
        Ok(Statement::Add(AddStmt { collection: Source { name, span }, record }))
    }

    /// The fields of a record, either `{ a: 1, b: 2 }` or one per line.
    fn record_body(&mut self) -> Result<Record, ParseError> {
        if *self.peek() == Tok::OpenCurly {
            return self.record_literal();
        }
        let mut out = Vec::new();
        loop {
            match self.peek() {
                Tok::Newline => {
                    self.advance();
                }
                Tok::Dedent | Tok::End => break,
                _ => {
                    let (name, _) = self.name("A field", "Write a field name, like 'id: 1'.")?;
                    self.expect(
                        Tok::Colon,
                        format!("'{name}' needs a value after it.").as_str(),
                        "Write a ':' and then the value.",
                    )?;
                    out.push((name, self.expr()?));
                    if !self.eat(&Tok::Comma) && !matches!(self.peek(), Tok::Newline | Tok::Dedent | Tok::End) {
                        break;
                    }
                }
            }
        }
        Ok(out)
    }

    fn record_literal(&mut self) -> Result<Record, ParseError> {
        self.advance(); // {
        let mut out = Vec::new();
        if *self.peek() != Tok::CloseCurly {
            loop {
                let (name, _) = self.name("A field", "Write a field name, like 'id: 1'.")?;
                self.expect(
                    Tok::Colon,
                    format!("'{name}' needs a value after it.").as_str(),
                    "Write a ':' and then the value.",
                )?;
                out.push((name, self.expr()?));
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(
            Tok::CloseCurly,
            "This record is missing its closing '}'.",
            "Add a '}' after the last field.",
        )?;
        Ok(out)
    }

    fn remove(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // remove
        let (name, span) = self.name("remove", "Write what to remove, like 'remove person'.")?;
        let mut if_exists = false;
        if self.at_word("if") && self.word_at(1, "exists") {
            self.advance();
            self.advance();
            if_exists = true;
        }
        Ok(Statement::Remove { name, if_exists, span })
    }

    /// Runs `inner` on the block indented under this line, or on the rest of
    /// this line when there is no block. Both forms mean the same thing.
    ///
    /// `inner` is told which one it got, because a pipeline with no source
    /// keeps going onto the next line only while a block is holding it.
    fn indented_or_inline<T>(
        &mut self,
        inner: impl FnOnce(&mut Self, bool) -> Result<T, ParseError>,
    ) -> Result<T, ParseError> {
        if *self.peek() == Tok::Newline && *self.peek_at(1) == Tok::Indent {
            self.advance();
            self.advance();
            let value = inner(self, true)?;
            while *self.peek() == Tok::Newline {
                self.advance();
            }
            self.eat(&Tok::Dedent);
            Ok(value)
        } else {
            inner(self, false)
        }
    }

    // --- Pipelines ----------------------------------------------------------

    /// `in_block` is true when an indented block is holding this pipeline, so
    /// a pipeline with no source may keep taking steps from the lines below
    /// it — that block's Dedent is what ends it.
    fn pipeline(&mut self, in_block: bool) -> Result<Pipeline, ParseError> {
        let source = match self.peek().clone() {
            Tok::Name(w) if !STEPS.contains(&w.as_str()) => {
                let span = self.span();
                self.advance();
                Some(Source { name: w, span })
            }
            Tok::Name(_) => None,
            _ => {
                return self.fail(
                    "A query starts with a collection or with a step.",
                    Some("Write a collection name, like 'person'."),
                )
            }
        };

        let mut steps = Vec::new();
        if source.is_none() {
            steps.push(self.step()?);
        }
        while self.eat(&Tok::Pipe) {
            steps.push(self.step()?);
        }

        if source.is_some() {
            // Steps indented under the collection they read from.
            if *self.peek() == Tok::Newline && *self.peek_at(1) == Tok::Indent {
                self.advance();
                self.advance();
                self.step_lines(&mut steps, true)?;
            }
        } else if in_block {
            // No source, so the steps are already at this block's depth and
            // simply continue down the page.
            self.step_lines(&mut steps, false)?;
        }

        Ok(Pipeline { source, steps })
    }

    /// Reads one line of steps after another until the block ends.
    fn step_lines(&mut self, steps: &mut Vec<Step>, eat_dedent: bool) -> Result<(), ParseError> {
        loop {
            match self.peek() {
                Tok::Dedent => {
                    if eat_dedent {
                        self.advance();
                    }
                    return Ok(());
                }
                Tok::End => return Ok(()),
                Tok::Newline => {
                    self.advance();
                }
                _ => {
                    steps.push(self.step()?);
                    while self.eat(&Tok::Pipe) {
                        steps.push(self.step()?);
                    }
                }
            }
        }
    }

    fn step(&mut self) -> Result<Step, ParseError> {
        let span = self.span();
        let word = match self.peek().clone() {
            Tok::Name(w) => w,
            _ => {
                return self.fail(
                    "A step has to start with a word.",
                    Some("The steps are where, show, sort, take, skip, unique, join, group by, follow, set and delete."),
                )
            }
        };
        self.advance();

        match word.as_str() {
            "where" => Ok(Step::Where(self.expr()?)),
            "show" => {
                let mut items = Vec::new();
                loop {
                    let item_span = self.span();
                    let alias = if matches!(self.peek(), Tok::Name(_))
                        && *self.peek_at(1) == Tok::Colon
                    {
                        let (a, _) = self.name("A field", "")?;
                        self.advance(); // :
                        Some(a)
                    } else {
                        None
                    };
                    items.push(ShowItem { alias, value: self.expr()?, span: item_span });
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                Ok(Step::Show(items))
            }
            "sort" => {
                let mut keys = Vec::new();
                loop {
                    let value = self.expr()?;
                    let direction = if self.eat_word("down") {
                        Direction::Down
                    } else {
                        self.eat_word("up");
                        Direction::Up
                    };
                    keys.push(SortKey { value, direction });
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                Ok(Step::Sort(keys))
            }
            "take" | "skip" => {
                let count = match self.peek().clone() {
                    Tok::Number(n) => {
                        self.advance();
                        n.as_i64().unwrap_or(0)
                    }
                    _ => {
                        return self.fail(
                            format!("'{word}' needs a count after it."),
                            Some(format!("Write a whole number, like '{word} 10'.").as_str()),
                        )
                    }
                };
                Ok(if word == "take" {
                    Step::Take { count, span }
                } else {
                    Step::Skip { count, span }
                })
            }
            "unique" => Ok(Step::Unique),
            "join" => {
                let (name, name_span) =
                    self.name("join", "Write the collection to join, like 'join pet on …'.")?;
                if !self.eat_word("on") {
                    return self.fail(
                        format!("'join {name}' needs an 'on' and a condition."),
                        Some("Write 'on' and then how the two sides match."),
                    );
                }
                let on = self.expr()?;
                let keep_all = if self.at_word("keep") && self.word_at(1, "all") {
                    self.advance();
                    self.advance();
                    true
                } else {
                    false
                };
                Ok(Step::Join(Join { collection: Source { name, span: name_span }, on, keep_all }))
            }
            "group" => {
                if !self.eat_word("by") {
                    return self.fail(
                        "'group' needs a 'by' after it.",
                        Some("Write 'group by species'."),
                    );
                }
                let mut keys = vec![self.expr()?];
                while self.eat(&Tok::Comma) {
                    keys.push(self.expr()?);
                }
                Ok(Step::GroupBy(keys))
            }
            "follow" => self.follow(false, span),
            "keep" => {
                if !self.eat_word("following") {
                    return self.fail(
                        "'keep' only makes sense as 'keep following'.",
                        Some("Write 'keep following knows' to follow a link all the way."),
                    );
                }
                self.follow(true, span)
            }
            "set" => {
                let mut out = Vec::new();
                loop {
                    let (field, field_span) =
                        self.name("set", "Write a field name, like 'set age = 31'.")?;
                    self.expect(
                        Tok::Equals,
                        format!("'set {field}' needs an '=' after it.").as_str(),
                        "Write '=' and then the new value.",
                    )?;
                    out.push(Assignment { field, value: self.expr()?, span: field_span });
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                Ok(Step::Set(out))
            }
            "delete" => Ok(Step::Delete { span }),
            _ => Ok(Step::Named(Source { name: word, span })),
        }
    }

    fn follow(&mut self, repeat: bool, span: Span) -> Result<Step, ParseError> {
        let (link, _) = self.name("follow", "Write the link to follow, like 'follow knows'.")?;
        let backward = self.eat_word("backward");
        Ok(Step::Follow(Follow { link, repeat, backward, span }))
    }

    // --- Expressions --------------------------------------------------------

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.and_expr()?;
        while self.at_word("or") {
            let span = self.span();
            self.advance();
            let right = self.and_expr()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.not_expr()?;
        while self.at_word("and") {
            let span = self.span();
            self.advance();
            let right = self.not_expr()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn not_expr(&mut self) -> Result<Expr, ParseError> {
        if self.at_word("not") {
            let span = self.span();
            self.advance();
            return Ok(Expr::Unary { op: UnOp::Not, value: Box::new(self.not_expr()?), span });
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Result<Expr, ParseError> {
        let first = self.additive()?;
        let span = self.span();

        // `x is None` / `x is not None`
        if self.at_word("is") {
            self.advance();
            let negated = self.eat_word("not");
            if !self.eat_word("None") {
                return self.fail(
                    "'is' is only used to ask about None.",
                    Some("Write 'is None' or 'is not None'. For values, use '=='."),
                );
            }
            return Ok(Expr::IsNone { value: Box::new(first), negated, span });
        }

        // `x in […]` / `x not in […]`
        let negated_in = self.at_word("not") && self.word_at(1, "in");
        if self.at_word("in") || negated_in {
            if negated_in {
                self.advance();
            }
            self.advance();
            let options = self.additive()?;
            return Ok(Expr::In {
                value: Box::new(first),
                options: Box::new(options),
                negated: negated_in,
                span,
            });
        }

        let mut rest = Vec::new();
        loop {
            let op = match self.peek() {
                Tok::EqualEqual => CompareOp::Equal,
                Tok::NotEqual => CompareOp::NotEqual,
                Tok::Less => CompareOp::Less,
                Tok::LessEqual => CompareOp::LessOrEqual,
                Tok::Greater => CompareOp::Greater,
                Tok::GreaterEqual => CompareOp::GreaterOrEqual,
                _ => break,
            };
            self.advance();
            rest.push((op, self.additive()?));
        }

        if rest.is_empty() {
            Ok(first)
        } else {
            Ok(Expr::Comparison { first: Box::new(first), rest, span })
        }
    }

    fn additive(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Subtract,
                _ => return Ok(left),
            };
            let span = self.span();
            self.advance();
            let right = self.multiplicative()?;
            left = Expr::Binary { left: Box::new(left), op, right: Box::new(right), span };
        }
    }

    fn multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Multiply,
                Tok::Slash => BinOp::Divide,
                Tok::Percent => BinOp::Remainder,
                _ => return Ok(left),
            };
            let span = self.span();
            self.advance();
            let right = self.unary()?;
            left = Expr::Binary { left: Box::new(left), op, right: Box::new(right), span };
        }
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if *self.peek() == Tok::Minus {
            let span = self.span();
            self.advance();
            return Ok(Expr::Unary { op: UnOp::Negate, value: Box::new(self.unary()?), span });
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let mut value = self.primary()?;
        loop {
            if self.eat(&Tok::Dot) {
                let (name, span) = self.name("A field", "Write a field or a method name.")?;
                if *self.peek() == Tok::OpenParen {
                    let args = self.arguments()?;
                    value = Expr::Method { value: Box::new(value), name, args, span };
                } else if let Expr::Field { mut path, span: field_span } = value {
                    path.push(name);
                    value = Expr::Field { path, span: field_span };
                } else {
                    return Err(ParseError {
                        message: format!("'{name}' cannot be read from this."),
                        span,
                        help: Some("A dotted name only reads fields of a record.".to_string()),
                    });
                }
            } else if *self.peek() == Tok::OpenSquare {
                let span = self.span();
                self.advance();
                let index = self.expr()?;
                self.expect(
                    Tok::CloseSquare,
                    "This lookup is missing its closing ']'.",
                    "Add a ']' after the position.",
                )?;
                value = Expr::Index { value: Box::new(value), index: Box::new(index), span };
            } else {
                return Ok(value);
            }
        }
    }

    fn arguments(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.advance(); // (
        let mut args = Vec::new();
        if *self.peek() != Tok::CloseParen {
            loop {
                args.push(self.expr()?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(
            Tok::CloseParen,
            "This call is missing its closing ')'.",
            "Add a ')' after the last value.",
        )?;
        Ok(args)
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let span = self.span();
        match self.peek().clone() {
            Tok::Number(n) => {
                self.advance();
                Ok(Expr::Literal { value: Literal::Number(n), span })
            }
            Tok::Text(s) => {
                self.advance();
                Ok(Expr::Literal { value: Literal::String(s), span })
            }
            Tok::OpenParen => {
                self.advance();
                let inner = self.expr()?;
                self.expect(
                    Tok::CloseParen,
                    "This group is missing its closing ')'.",
                    "Add a ')' to close it.",
                )?;
                Ok(inner)
            }
            Tok::OpenSquare => {
                self.advance();
                let mut items = Vec::new();
                if *self.peek() != Tok::CloseSquare {
                    loop {
                        items.push(self.expr()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                }
                self.expect(
                    Tok::CloseSquare,
                    "This list is missing its closing ']'.",
                    "Add a ']' after the last value.",
                )?;
                Ok(Expr::List { items, span })
            }
            Tok::OpenCurly => Ok(Expr::RecordLiteral { fields: self.record_literal()?, span }),
            Tok::Name(w) => {
                self.advance();
                match w.as_str() {
                    "True" => return Ok(Expr::Literal { value: Literal::Bool(true), span }),
                    "False" => return Ok(Expr::Literal { value: Literal::Bool(false), span }),
                    "None" => return Ok(Expr::Literal { value: Literal::None, span }),
                    _ => {}
                }
                if *self.peek() == Tok::OpenParen {
                    let args = self.arguments()?;
                    Ok(Expr::Call { name: w, args, span })
                } else {
                    Ok(Expr::Field { path: vec![w], span })
                }
            }
            _ => self.fail(
                "A value is missing here.",
                Some("Write a field name, a number, or some text."),
            ),
        }
    }
}
