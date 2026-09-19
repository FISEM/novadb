//! The shape of a parsed nova program.
//!
//! Nothing here resolves anything. A name is a name: whether it turns out to
//! be a collection, a piece defined with `define … as`, or nothing at all is
//! the engine's problem, not the grammar's.

use serde::{Deserialize, Serialize};
use serde_json::Number;

/// Where a piece of text sits in the source, so an error can point at it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// Two spans always compare equal, so that comparing two trees compares what
/// they mean and not where they were written. `person | where age > 30` and
/// the same query written across three indented lines are the same tree, and
/// tests get to say so directly. Check a span by reading `start` and `end`.
impl PartialEq for Span {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for Span {}

// --- Statements -------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    /// A pipeline run on its own. Reads, unless its last step writes.
    Query(Pipeline),
    /// `define person` — states the shape of a collection's records.
    DefineShape(ShapeDef),
    /// `define adults as …` — names a piece of a pipeline.
    DefineName(NameDef),
    /// `add person { … }`
    Add(AddStmt),
    /// `remove person [if exists]`
    Remove { name: String, if_exists: bool, span: Span },
}

// --- Pipelines --------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pipeline {
    /// The name the records come from.
    ///
    /// `None` when the pipeline opens with a step instead — the shape a
    /// `define … as` takes when it is meant to be piped into.
    pub source: Option<Source>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Step {
    Where(Expr),
    Show(Vec<ShowItem>),
    Sort(Vec<SortKey>),
    Take { count: i64, span: Span },
    Skip { count: i64, span: Span },
    Unique,
    Join(Join),
    GroupBy(Vec<Expr>),
    Follow(Follow),
    Set(Vec<Assignment>),
    Delete { span: Span },
    /// A bare name in step position: a piece defined with `define … as`.
    Named(Source),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowItem {
    /// The name the value takes in the output. `None` for a bare field, where
    /// the field keeps its own name.
    pub alias: Option<String>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortKey {
    pub value: Expr,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Join {
    pub collection: Source,
    pub on: Expr,
    /// `keep all`: records with no match come through anyway.
    pub keep_all: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Follow {
    /// The label on the link, as written: `follow knows`.
    pub link: String,
    /// `keep following`: as far as the links go, rather than one step.
    pub repeat: bool,
    /// `backward`: against the arrow.
    pub backward: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub field: String,
    pub value: Expr,
    pub span: Span,
}

// --- Shapes and names -------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeDef {
    pub name: String,
    /// `None` when `define` had no body: the collection makes no claim about
    /// its records, and every one may differ. An empty `Some` is a body that
    /// happens to declare nothing, which is a different statement.
    pub fields: Option<Vec<FieldDef>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub kind: TypeName,
    /// Written with a trailing `?`.
    pub optional: bool,
    /// Written with `key`.
    pub key: bool,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeName {
    Number,
    String,
    Boolean,
    Any,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NameDef {
    pub name: String,
    pub pipeline: Pipeline,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddStmt {
    pub collection: Source,
    pub record: Record,
}

/// Field names to values, in the order they were written.
pub type Record = Vec<(String, Expr)>;

// --- Expressions ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Literal { value: Literal, span: Span },
    /// A dotted path exactly as written: `name`, `person.name`, `p.address.city`.
    /// Whether the first piece names a collection or a field is resolved later.
    Field { path: Vec<String>, span: Span },
    Binary { left: Box<Expr>, op: BinOp, right: Box<Expr>, span: Span },
    Unary { op: UnOp, value: Box<Expr>, span: Span },
    /// One or more comparisons in a row, so `18 < age < 65` keeps its shape
    /// instead of being rewritten into an `and` that reads the middle twice.
    /// A plain `a > b` is this with one entry in `rest`.
    Comparison { first: Box<Expr>, rest: Vec<(CompareOp, Expr)>, span: Span },
    /// `len(name)`, `count()`, `highest(age)`.
    Call { name: String, args: Vec<Expr>, span: Span },
    /// `name.upper()`, `name.startswith("a")`.
    Method { value: Box<Expr>, name: String, args: Vec<Expr>, span: Span },
    /// `tags[0]`, `payload["items"]`.
    Index { value: Box<Expr>, index: Box<Expr>, span: Span },
    /// `x in […]`, `x not in […]`.
    In { value: Box<Expr>, options: Box<Expr>, negated: bool, span: Span },
    /// `x is None`, `x is not None`.
    IsNone { value: Box<Expr>, negated: bool, span: Span },
    List { items: Vec<Expr>, span: Span },
    RecordLiteral { fields: Record, span: Span },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    /// Kept as a JSON number, which is what the engine stores. nova has one
    /// `number` type for the same reason.
    Number(Number),
    String(String),
    Bool(bool),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnOp {
    Not,
    Negate,
}
