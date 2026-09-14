use std::path::Path;

use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

const CATALOG: TableDefinition<&str, &str> = TableDefinition::new("catalog");
const SEQUENCES: TableDefinition<&str, u64> = TableDefinition::new("sequences");
const ROWS: TableDefinition<&str, &str> = TableDefinition::new("rows");

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Db(#[from] redb::DatabaseError),
    #[error("transaction error: {0}")]
    Transaction(#[from] redb::TransactionError),
    #[error("table error: {0}")]
    Table(#[from] redb::TableError),
    #[error("storage error: {0}")]
    Storage(#[from] redb::StorageError),
    #[error("commit error: {0}")]
    Commit(#[from] redb::CommitError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("table '{0}' already exists")]
    TableAlreadyExists(String),
    #[error("table '{0}' does not exist")]
    TableNotFound(String),
    #[error("row {1} not found in table '{0}'")]
    RowNotFound(String, u64),
}

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    pub data_type: String,
    pub primary_key: bool,
    pub not_null: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<ColumnSchema>,
}

pub struct Store {
    db: Database,
}

fn row_key(table: &str, id: u64) -> String {
    format!("{table}\0{id:020}")
}

fn row_prefix_bounds(table: &str) -> (String, String) {
    (format!("{table}\0"), format!("{table}\u{1}"))
}

fn id_from_key(key: &str, table: &str) -> u64 {
    let (_, id_str) = key.split_at(table.len() + 1);
    id_str.parse().unwrap_or(0)
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::create(path)?;
        // Ensure tables exist.
        let txn = db.begin_write()?;
        {
            txn.open_table(CATALOG)?;
            txn.open_table(SEQUENCES)?;
            txn.open_table(ROWS)?;
        }
        txn.commit()?;
        Ok(Store { db })
    }

    pub fn create_table(&self, schema: &TableSchema, if_not_exists: bool) -> Result<bool> {
        let txn = self.db.begin_write()?;
        {
            let mut catalog = txn.open_table(CATALOG)?;
            if catalog.get(schema.name.as_str())?.is_some() {
                if if_not_exists {
                    return Ok(false);
                }
                return Err(StorageError::TableAlreadyExists(schema.name.clone()));
            }
            let json = serde_json::to_string(schema)?;
            catalog.insert(schema.name.as_str(), json.as_str())?;
        }
        {
            let mut sequences = txn.open_table(SEQUENCES)?;
            sequences.insert(schema.name.as_str(), 0u64)?;
        }
        txn.commit()?;
        Ok(true)
    }

    pub fn drop_table(&self, name: &str, if_exists: bool) -> Result<bool> {
        let txn = self.db.begin_write()?;
        let existed;
        {
            let mut catalog = txn.open_table(CATALOG)?;
            existed = catalog.remove(name)?.is_some();
            if !existed && !if_exists {
                return Err(StorageError::TableNotFound(name.to_string()));
            }
        }
        {
            let mut sequences = txn.open_table(SEQUENCES)?;
            sequences.remove(name)?;
        }
        {
            let mut rows = txn.open_table(ROWS)?;
            let (start, end) = row_prefix_bounds(name);
            let keys: Vec<String> = rows
                .range(start.as_str()..end.as_str())?
                .map(|entry| entry.map(|(k, _)| k.value().to_string()))
                .collect::<std::result::Result<_, _>>()?;
            for k in keys {
                rows.remove(k.as_str())?;
            }
        }
        txn.commit()?;
        Ok(existed)
    }

    pub fn get_schema(&self, name: &str) -> Result<Option<TableSchema>> {
        let txn = self.db.begin_read()?;
        let catalog = txn.open_table(CATALOG)?;
        match catalog.get(name)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    pub fn list_tables(&self) -> Result<Vec<String>> {
        let txn = self.db.begin_read()?;
        let catalog = txn.open_table(CATALOG)?;
        let mut names = Vec::new();
        for entry in catalog.iter()? {
            let (k, _) = entry?;
            names.push(k.value().to_string());
        }
        Ok(names)
    }

    fn require_table(&self, name: &str) -> Result<()> {
        if self.get_schema(name)?.is_none() {
            return Err(StorageError::TableNotFound(name.to_string()));
        }
        Ok(())
    }

    pub fn insert_row(&self, table: &str, row: Map<String, Value>) -> Result<u64> {
        self.require_table(table)?;
        let txn = self.db.begin_write()?;
        let id;
        {
            let mut sequences = txn.open_table(SEQUENCES)?;
            let next = sequences.get(table)?.map(|v| v.value()).unwrap_or(0) + 1;
            sequences.insert(table, next)?;
            id = next;
        }
        {
            let mut rows = txn.open_table(ROWS)?;
            let key = row_key(table, id);
            let json = serde_json::to_string(&Value::Object(row))?;
            rows.insert(key.as_str(), json.as_str())?;
        }
        txn.commit()?;
        Ok(id)
    }

    pub fn scan_table(&self, table: &str) -> Result<Vec<(u64, Map<String, Value>)>> {
        self.require_table(table)?;
        let txn = self.db.begin_read()?;
        let rows = txn.open_table(ROWS)?;
        let (start, end) = row_prefix_bounds(table);
        let mut out = Vec::new();
        for entry in rows.range(start.as_str()..end.as_str())? {
            let (k, v) = entry?;
            let id = id_from_key(k.value(), table);
            let value: Value = serde_json::from_str(v.value())?;
            if let Value::Object(map) = value {
                out.push((id, map));
            }
        }
        Ok(out)
    }

    pub fn update_row(&self, table: &str, id: u64, row: Map<String, Value>) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let mut rows = txn.open_table(ROWS)?;
            let key = row_key(table, id);
            if rows.get(key.as_str())?.is_none() {
                return Err(StorageError::RowNotFound(table.to_string(), id));
            }
            let json = serde_json::to_string(&Value::Object(row))?;
            rows.insert(key.as_str(), json.as_str())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn delete_row(&self, table: &str, id: u64) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let mut rows = txn.open_table(ROWS)?;
            let key = row_key(table, id);
            rows.remove(key.as_str())?;
        }
        txn.commit()?;
        Ok(())
    }
}
