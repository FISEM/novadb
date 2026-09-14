#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    CreateTable(CreateTableStmt),
    DropTable { name: String, if_exists: bool },
    Insert(InsertStmt),
    Select(SelectStmt),
    Update(UpdateStmt),
    Delete(DeleteStmt),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateTableStmt {
    pub name: String,
    pub if_not_exists: bool,
    pub columns: Vec<ColumnDef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: DataType,
    pub primary_key: bool,
    pub not_null: bool,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Integer,
    BigInt,
    Double,
    Text,
    Boolean,
    Json,
    Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InsertStmt {
    pub table: String,
    pub columns: Option<Vec<String>>,
    pub values: Vec<Vec<Expr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateStmt {
    pub table: String,
    pub assignments: Vec<(String, Expr)>,
    pub filter: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteStmt {
    pub table: String,
    pub filter: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectStmt {
    pub ctes: Vec<CteDef>,
    pub body: SetExpr,
    pub order_by: Vec<OrderByItem>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CteDef {
    pub name: String,
    /// Optional explicit column list, e.g. `WITH reachable(id) AS (...)`.
    /// Parsed for standard-SQL compatibility; the executor expects the
    /// inner query to already project columns under their final names.
    pub columns: Option<Vec<String>>,
    pub recursive: bool,
    pub query: Box<SetExpr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SetExpr {
    Select(Box<SelectCore>),
    Union { left: Box<SetExpr>, all: bool, right: Box<SetExpr> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectCore {
    pub distinct: bool,
    pub projection: Vec<SelectItem>,
    pub from: Option<TableRef>,
    pub joins: Vec<JoinClause>,
    pub filter: Option<Expr>,
    pub group_by: Vec<Expr>,
    pub having: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectItem {
    Wildcard,
    Expr { expr: Expr, alias: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableRef {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JoinClause {
    pub kind: JoinKind,
    pub table: TableRef,
    pub on: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JoinKind {
    Inner,
    Left,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderByItem {
    pub expr: Expr,
    pub desc: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Column(ColumnRef),
    BinaryOp { left: Box<Expr>, op: BinOp, right: Box<Expr> },
    UnaryOp { op: UnOp, expr: Box<Expr> },
    IsNull { expr: Box<Expr>, negated: bool },
    InList { expr: Box<Expr>, list: Vec<Expr>, negated: bool },
    Between { expr: Box<Expr>, low: Box<Expr>, high: Box<Expr>, negated: bool },
    FunctionCall { name: String, args: Vec<Expr> },
    CountStar,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnRef {
    pub table: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
    Concat,
    Like,
    JsonGet,
    JsonGetText,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnOp {
    Not,
    Neg,
}
