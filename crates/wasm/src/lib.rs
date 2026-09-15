use engine::Database;
use serde_json::json;
use wasm_bindgen::prelude::*;

/// A novadb instance backed entirely by in-memory storage, exposed to
/// JavaScript for a browser playground — no server, no filesystem, one
/// isolated database per page load.
#[wasm_bindgen]
pub struct NovaDb {
    db: Database,
}

#[wasm_bindgen]
impl NovaDb {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<NovaDb, JsValue> {
        Database::open_in_memory().map(|db| NovaDb { db }).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Runs one or more semicolon-separated SQL statements and returns a
    /// JSON string shaped exactly like the HTTP server's `/sql` response:
    /// `[{"status":"OK","result":...}, ...]` or `[{"status":"ERR","detail":...}]`.
    pub fn execute(&self, sql: &str) -> String {
        let body = match self.db.execute(sql) {
            Ok(results) => {
                let json_results: Vec<_> =
                    results.iter().map(|r| json!({ "status": "OK", "result": r })).collect();
                json!(json_results)
            }
            Err(e) => json!([{ "status": "ERR", "detail": e.to_string() }]),
        };
        serde_json::to_string(&body).unwrap_or_default()
    }

    /// Returns a JSON array of table names currently in the catalog.
    pub fn list_tables(&self) -> String {
        let names = self.db.list_tables().unwrap_or_default();
        serde_json::to_string(&names).unwrap_or_default()
    }
}
