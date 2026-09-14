use std::io::{self, Write};

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "novadb", about = "novadb SQL client REPL")]
struct Args {
    /// Base URL of the novadb server.
    #[arg(long, default_value = "http://127.0.0.1:8801")]
    url: String,
}

fn main() {
    let args = Args::parse();
    println!("novadb client — connected to {}", args.url);
    println!("Type standard SQL statements ending with ';'. Type 'exit' to quit.\n");

    let mut buffer = String::new();
    loop {
        let prompt = if buffer.is_empty() { "sql> " } else { "  -> " };
        print!("{prompt}");
        io::stdout().flush().ok();

        let mut line = String::new();
        if io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
            break; // EOF
        }
        let trimmed = line.trim();
        if buffer.is_empty() && (trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit")) {
            break;
        }
        buffer.push_str(&line);
        if !trimmed.ends_with(';') {
            continue;
        }

        let query = std::mem::take(&mut buffer);
        run_query(&args.url, &query);
    }
}

fn run_query(base_url: &str, query: &str) {
    let endpoint = format!("{}/sql", base_url.trim_end_matches('/'));
    match ureq::post(&endpoint).send_string(query) {
        Ok(response) => match response.into_json::<serde_json::Value>() {
            Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default()),
            Err(e) => eprintln!("failed to parse response: {e}"),
        },
        Err(ureq::Error::Status(_, response)) => match response.into_json::<serde_json::Value>() {
            Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap_or_default()),
            Err(e) => eprintln!("request failed: {e}"),
        },
        Err(e) => eprintln!("connection error: {e}"),
    }
}
