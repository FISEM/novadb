use std::io::{self, Write};

use clap::Parser;
use serde_json::Value;

#[derive(Parser, Debug)]
#[command(name = "novadb", about = "novadb client: writes shutup, reads records")]
struct Args {
    /// Base URL of the novadb server.
    #[arg(long, default_value = "http://127.0.0.1:8801")]
    url: String,
}

fn main() {
    let args = Args::parse();
    println!("novadb — connected to {}", args.url);
    println!("Write shutup. A blank line runs what you have written.");
    println!("Indentation matters, so a query can span as many lines as it needs.");
    println!("Type 'exit' to quit.\n");

    let mut buffer = String::new();
    loop {
        print!("{}", if buffer.is_empty() { "shutup> " } else { "      | " });
        io::stdout().flush().ok();

        let mut line = String::new();
        if io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
            break; // end of input
        }
        let trimmed = line.trim();

        if buffer.is_empty() {
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.eq_ignore_ascii_case("exit") || trimmed.eq_ignore_ascii_case("quit") {
                break;
            }
        }

        // A blank line is what says "that is the whole query" — nothing else
        // can, because a step on the next line is still part of this one.
        if trimmed.is_empty() {
            let source = std::mem::take(&mut buffer);
            send(&args.url, &source);
            println!();
            continue;
        }
        buffer.push_str(&line);
    }
}

fn send(base_url: &str, source: &str) {
    let endpoint = format!("{}/run", base_url.trim_end_matches('/'));
    let reply = match ureq::post(&endpoint).send_string(source) {
        Ok(response) | Err(ureq::Error::Status(_, response)) => response.into_json::<Value>(),
        Err(e) => {
            eprintln!("cannot reach {endpoint}: {e}");
            return;
        }
    };
    match reply {
        Ok(Value::Array(entries)) => {
            for entry in entries {
                report(&entry, source);
            }
        }
        Ok(other) => println!("{other}"),
        Err(e) => eprintln!("the server said something unreadable: {e}"),
    }
}

fn report(entry: &Value, source: &str) {
    if entry.get("status").and_then(Value::as_str) == Some("ERR") {
        show_error(entry, source);
        return;
    }
    match entry.get("result") {
        Some(result) => show_result(result),
        None => println!("{entry}"),
    }
}

fn show_result(result: &Value) {
    let kind = result.get("type").and_then(Value::as_str).unwrap_or("");
    let count = |field: &str| result.get(field).and_then(Value::as_u64).unwrap_or(0);
    match kind {
        "select" => match result.get("rows").and_then(Value::as_array) {
            Some(rows) => show_table(rows),
            None => println!("{result}"),
        },
        "inserted" => {
            let n = result.get("ids").and_then(Value::as_array).map_or(0, Vec::len);
            println!("Added {n} record{}.", plural(n as u64));
        }
        "updated" => println!("Changed {} record{}.", count("count"), plural(count("count"))),
        "deleted" => println!("Removed {} record{}.", count("count"), plural(count("count"))),
        "created_table" => println!("Defined {}.", named(result, "table")),
        "dropped_table" => println!("Removed {}.", named(result, "table")),
        _ => println!("{result}"),
    }
}

fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn named(result: &Value, field: &str) -> String {
    result.get(field).and_then(Value::as_str).unwrap_or("it").to_string()
}

/// Records as a plain table: columns in the order the query asked for them.
fn show_table(rows: &[Value]) {
    if rows.is_empty() {
        println!("No records.");
        return;
    }

    let mut columns: Vec<String> = Vec::new();
    for row in rows {
        if let Some(record) = row.as_object() {
            for key in record.keys() {
                if !columns.iter().any(|c| c == key) {
                    columns.push(key.clone());
                }
            }
        }
    }

    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| columns.iter().map(|c| cell(row.get(c))).collect())
        .collect();

    let widths: Vec<usize> = columns
        .iter()
        .enumerate()
        .map(|(i, name)| {
            cells.iter().map(|r| r[i].chars().count()).chain([name.chars().count()]).max().unwrap_or(0)
        })
        .collect();

    let line = |values: &[String]| {
        let row: Vec<String> = values
            .iter()
            .zip(&widths)
            .map(|(v, w)| format!("{v:<w$}", w = *w))
            .collect();
        println!("{}", row.join("  ").trim_end());
    };

    line(&columns);
    println!("{}", widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("  "));
    for row in &cells {
        line(row);
    }
    println!("{} record{}.", rows.len(), plural(rows.len() as u64));
}

fn cell(value: Option<&Value>) -> String {
    match value {
        None => "—".to_string(),
        Some(Value::Null) => "None".to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// The error contract, in a terminal: point at the text, say what is wrong,
/// name the fix.
fn show_error(entry: &Value, source: &str) {
    if let Some(start) = entry.get("start").and_then(Value::as_u64).map(|n| n as usize) {
        let end = entry.get("end").and_then(Value::as_u64).map_or(start, |n| n as usize);
        let line_start = source[..start.min(source.len())].rfind('\n').map_or(0, |at| at + 1);
        let line_end = source[line_start..].find('\n').map_or(source.len(), |at| line_start + at);
        let line = &source[line_start..line_end];
        let column = start.saturating_sub(line_start);
        let width = end.saturating_sub(start).max(1).min(line.len().saturating_sub(column).max(1));
        println!("{line}");
        println!("{}{}", " ".repeat(column), "^".repeat(width));
    }
    if let Some(detail) = entry.get("detail").and_then(Value::as_str) {
        println!("{detail}");
    }
    if let Some(help) = entry.get("help").and_then(Value::as_str) {
        println!("{help}");
    }
}
