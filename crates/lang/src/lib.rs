//! shutup — novadb's query language.
//!
//! This crate is at the Types step: the shapes and the signatures are fixed,
//! and nothing is implemented. See `docs/language.md` for the reference and
//! `docs/design-notes.md` for why it is shaped this way.

pub mod ast;
mod lexer;
mod parser;

pub use ast::*;

use thiserror::Error;

/// What a failed parse says.
///
/// The three fields are the error contract from the reference: point at the
/// exact text, say what is wrong in a full sentence, and name the fix. `help`
/// is optional because a few errors genuinely have no next step to suggest —
/// but most do, and an error without one should be treated as unfinished.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct ParseError {
    /// A full sentence, not a fragment. "person has no field 'aeg'."
    pub message: String,
    /// The text to underline.
    pub span: Span,
    /// The fix, as an instruction. "Move 'sort age' before it, or add age to
    /// the show."
    pub help: Option<String>,
}

/// Reads shutup source into statements.
///
/// Statements are separated by newlines or `;`. Empty input is no statements,
/// not an error.
pub fn parse(source: &str) -> Result<Vec<Statement>, ParseError> {
    parser::parse_tokens(lexer::tokenize(source)?)
}
