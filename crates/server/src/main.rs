use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use engine::Database;
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(name = "novadb-server", about = "novadb: a standard-SQL database engine with SurrealDB-like capabilities")]
struct Args {
    /// Path to the database file on disk.
    #[arg(long, default_value = "novadb.redb")]
    data_file: String,

    /// Address to bind the HTTP API to.
    #[arg(long, default_value = "127.0.0.1:8801")]
    bind: String,
}

struct AppState {
    db: Database,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let db = Database::open(&args.data_file).unwrap_or_else(|e| {
        eprintln!("failed to open database at {}: {e}", args.data_file);
        std::process::exit(1);
    });

    let state = Arc::new(AppState { db });

    let app = Router::new()
        .route("/health", get(health))
        .route("/sql", post(run_sql))
        .with_state(state);

    let addr: SocketAddr = args.bind.parse().unwrap_or_else(|_| {
        eprintln!("invalid bind address: {}", args.bind);
        std::process::exit(1);
    });

    tracing::info!("novadb listening on http://{addr}  (POST SQL text to /sql)");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> &'static str {
    "ok"
}

async fn run_sql(State(state): State<Arc<AppState>>, body: String) -> (StatusCode, Json<Value>) {
    match state.db.execute(&body) {
        Ok(results) => {
            let json_results: Vec<Value> = results
                .iter()
                .map(|r| json!({ "status": "OK", "result": r }))
                .collect();
            (StatusCode::OK, Json(Value::Array(json_results)))
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!([{ "status": "ERR", "detail": e.to_string() }])),
        ),
    }
}
