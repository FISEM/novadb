mod exec;
mod nova;
mod value;

use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};
use sql::ast::*;
use thiserror::Error;

pub use storage::StorageError;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("parse error: {0}")]
    Parse(#[from] sql::ParseError),
    #[error("{0}")]
    Nova(#[from] lang::ParseError),
    #[error(transparent)]
    Storage(#[from] storage::StorageError),
    #[error("unknown table '{0}'")]
    UnknownTable(String),
    #[error("column count does not match value count for table '{0}'")]
    ColumnCountMismatch(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ExecResult {
    #[serde(rename = "select")]
    Select { rows: Vec<Map<String, Value>> },
    #[serde(rename = "inserted")]
    Inserted { ids: Vec<u64> },
    #[serde(rename = "updated")]
    Updated { count: u64 },
    #[serde(rename = "deleted")]
    Deleted { count: u64 },
    #[serde(rename = "created_table")]
    CreatedTable { table: String, created: bool },
    #[serde(rename = "dropped_table")]
    DroppedTable { table: String, existed: bool },
}

pub struct Database {
    store: storage::Store,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Database { store: storage::Store::open(path)? })
    }

    pub fn open_in_memory() -> Result<Self> {
        Ok(Database { store: storage::Store::open_in_memory()? })
    }

    pub fn list_tables(&self) -> Result<Vec<String>> {
        Ok(self.store.list_tables()?)
    }

    /// Runs nova source: novadb's own query language.
    pub fn run(&self, source: &str) -> Result<Vec<ExecResult>> {
        let statements = lang::parse(source)?;
        let mut results = Vec::with_capacity(statements.len());
        for stmt in &statements {
            results.push(nova::run_statement(&self.store, stmt)?);
        }
        Ok(results)
    }

    pub fn execute(&self, sql_text: &str) -> Result<Vec<ExecResult>> {
        let statements = sql::parse_statements(sql_text)?;
        let mut results = Vec::with_capacity(statements.len());
        for stmt in &statements {
            results.push(self.execute_statement(stmt)?);
        }
        Ok(results)
    }

    fn execute_statement(&self, stmt: &Statement) -> Result<ExecResult> {
        match stmt {
            Statement::CreateTable(create) => self.exec_create_table(create),
            Statement::DropTable { name, if_exists } => self.exec_drop_table(name, *if_exists),
            Statement::Insert(insert) => self.exec_insert(insert),
            Statement::Select(select) => self.exec_select(select),
            Statement::Update(update) => self.exec_update(update),
            Statement::Delete(delete) => self.exec_delete(delete),
        }
    }

    fn exec_create_table(&self, create: &CreateTableStmt) -> Result<ExecResult> {
        let columns = create
            .columns
            .iter()
            .map(|c| storage::ColumnSchema {
                name: c.name.clone(),
                data_type: format!("{:?}", c.data_type),
                primary_key: c.primary_key,
                not_null: c.not_null,
            })
            .collect();
        let schema = storage::TableSchema { name: create.name.clone(), columns };
        let created = self.store.create_table(&schema, create.if_not_exists)?;
        Ok(ExecResult::CreatedTable { table: create.name.clone(), created })
    }

    fn exec_drop_table(&self, name: &str, if_exists: bool) -> Result<ExecResult> {
        let existed = self.store.drop_table(name, if_exists)?;
        Ok(ExecResult::DroppedTable { table: name.to_string(), existed })
    }

    fn exec_insert(&self, insert: &InsertStmt) -> Result<ExecResult> {
        let schema = self
            .store
            .get_schema(&insert.table)?
            .ok_or_else(|| EngineError::UnknownTable(insert.table.clone()))?;

        let column_names: Vec<String> = match &insert.columns {
            Some(cols) => cols.clone(),
            None => schema.columns.iter().map(|c| c.name.clone()).collect(),
        };

        let mut ids = Vec::with_capacity(insert.values.len());
        let empty_row = exec::Row::empty();
        for value_row in &insert.values {
            if value_row.len() != column_names.len() {
                return Err(EngineError::ColumnCountMismatch(insert.table.clone()));
            }
            let mut map = Map::new();
            for (name, expr) in column_names.iter().zip(value_row.iter()) {
                map.insert(name.clone(), exec::eval_expr(&empty_row, expr)?);
            }
            let id = self.store.insert_row(&insert.table, map)?;
            ids.push(id);
        }
        Ok(ExecResult::Inserted { ids })
    }

    fn exec_select(&self, select: &SelectStmt) -> Result<ExecResult> {
        let mut ctx = exec::ExecCtx::new(&self.store);
        let rows = ctx.eval_select_stmt(select)?;
        Ok(ExecResult::Select { rows })
    }

    fn exec_update(&self, update: &UpdateStmt) -> Result<ExecResult> {
        self.store
            .get_schema(&update.table)?
            .ok_or_else(|| EngineError::UnknownTable(update.table.clone()))?;

        let rows = self.store.scan_table(&update.table)?;
        let mut count = 0u64;
        for (id, mut row) in rows {
            row.entry("id".to_string()).or_insert(Value::Number(id.into()));
            let matches = match &update.filter {
                Some(filter) => {
                    let r = exec::Row::single(update.table.clone(), row.clone());
                    value::is_truthy(&exec::eval_expr(&r, filter)?)
                }
                None => true,
            };
            if !matches {
                continue;
            }
            let eval_row = exec::Row::single(update.table.clone(), row.clone());
            for (col, expr) in &update.assignments {
                let v = exec::eval_expr(&eval_row, expr)?;
                row.insert(col.clone(), v);
            }
            self.store.update_row(&update.table, id, row)?;
            count += 1;
        }
        Ok(ExecResult::Updated { count })
    }

    fn exec_delete(&self, delete: &DeleteStmt) -> Result<ExecResult> {
        self.store
            .get_schema(&delete.table)?
            .ok_or_else(|| EngineError::UnknownTable(delete.table.clone()))?;

        let rows = self.store.scan_table(&delete.table)?;
        let mut count = 0u64;
        for (id, mut row) in rows {
            row.entry("id".to_string()).or_insert(Value::Number(id.into()));
            let matches = match &delete.filter {
                Some(filter) => {
                    let r = exec::Row::single(delete.table.clone(), row);
                    value::is_truthy(&exec::eval_expr(&r, filter)?)
                }
                None => true,
            };
            if matches {
                self.store.delete_row(&delete.table, id)?;
                count += 1;
            }
        }
        Ok(ExecResult::Deleted { count })
    }
}
