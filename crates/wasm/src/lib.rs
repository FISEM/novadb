use engine::{Database, EngineError};
use serde_json::json;
use wasm_bindgen::prelude::*;

/// A novadb instance held entirely in memory and handed to JavaScript, so a
/// browser can run shutup with no server and no filesystem — one database
/// per page load.
#[wasm_bindgen]
pub struct NovaDb {
    db: Database,
}

#[wasm_bindgen]
impl NovaDb {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<NovaDb, JsValue> {
        Database::open_in_memory()
            .map(|db| NovaDb { db })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Runs shutup source and returns JSON shaped like the HTTP server's
    /// reply: `[{"status":"OK","result":…}, …]`, or a single `ERR` carrying
    /// the message, the fix, and where in the source to point.
    pub fn run(&self, source: &str) -> String {
        let body = match self.db.run(source) {
            Ok(results) => {
                let each: Vec<_> =
                    results.iter().map(|r| json!({ "status": "OK", "result": r })).collect();
                json!(each)
            }
            // A parse error knows where it is and what to do about it. Losing
            // that on the way to the screen would throw away the part of the
            // error that helps.
            Err(EngineError::Shutup(parse)) => json!([{
                "status": "ERR",
                "detail": parse.message,
                "help": parse.help,
                "start": parse.span.start,
                "end": parse.span.end,
            }]),
            Err(other) => json!([{ "status": "ERR", "detail": other.to_string() }]),
        };
        serde_json::to_string(&body).unwrap_or_default()
    }

    /// The names of the collections that exist, as a JSON array.
    pub fn collections(&self) -> String {
        let names = self.db.list_tables().unwrap_or_default();
        serde_json::to_string(&names).unwrap_or_default()
    }
}
