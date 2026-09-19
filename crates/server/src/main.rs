use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use engine::{Database, EngineError};
use serde_json::{json, Value};

#[derive(Parser, Debug)]
#[command(name = "novadb-server", about = "novadb: relational, document and graph in one pipeline")]
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
        .route("/run", post(run))
        .with_state(state);

    let addr: SocketAddr = args.bind.parse().unwrap_or_else(|_| {
        eprintln!("invalid bind address: {}", args.bind);
        std::process::exit(1);
    });

    tracing::info!("novadb listening on http://{addr}  (POST shutup source to /run)");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> &'static str {
    "ok"
}

/// Runs shutup source. One result per statement, or a single error carrying
/// the message, the fix, and where in the source to point — the same shape
/// the browser playground reads.
async fn run(State(state): State<Arc<AppState>>, body: String) -> (StatusCode, Json<Value>) {
    match state.db.run(&body) {
        Ok(results) => {
            let each: Vec<Value> =
                results.iter().map(|r| json!({ "status": "OK", "result": r })).collect();
            (StatusCode::OK, Json(Value::Array(each)))
        }
        Err(EngineError::Shutup(parse)) => (
            StatusCode::BAD_REQUEST,
            Json(json!([{
                "status": "ERR",
                "detail": parse.message,
                "help": parse.help,
                "start": parse.span.start,
                "end": parse.span.end,
            }])),
        ),
        Err(other) => (
            StatusCode::BAD_REQUEST,
            Json(json!([{ "status": "ERR", "detail": other.to_string() }])),
        ),
    }
}
