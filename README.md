# novadb

A SQL database engine aiming to cover SurrealDB-like document + graph
capabilities while staying on **standard SQL** — no proprietary query
language, so existing SQL knowledge and tooling carry over directly.

## Architecture

Five crates in a Cargo workspace:

- [`crates/sql`](crates/sql/src) — lexer, parser, and AST for the SQL
  dialect (see below).
- [`crates/storage`](crates/storage/src) — key-value storage backend on top
  of [redb](https://github.com/cberner/redb). Tables are schemas stored in
  a catalog; rows are JSON documents keyed by an auto-incrementing id.
- [`crates/engine`](crates/engine/src) — executes parsed statements against
  storage: joins, `GROUP BY`/aggregates, recursive CTEs, scalar functions.
- [`crates/server`](crates/server/src) — HTTP server (axum) exposing
  `POST /sql`: send raw SQL text, get JSON back. No custom wire protocol.
- [`crates/cli`](crates/cli/src) — a REPL client that talks to the server
  over HTTP.

### Why standard SQL, not a custom language

Implementing a Postgres-wire-compatible protocol (auth, extended query
protocol, type OIDs) was considered and deliberately deferred: it's a large,
orthogonal undertaking that doesn't contribute to the project's actual
value. The API surface is plain HTTP/JSON instead, and a pg-wire adapter can
be layered on later once the core engine is stable.

Similarly, the graph capability is *not* a custom grammar (unlike
SurrealDB's `->edge->table` traversal syntax) — it's modeled as ordinary
edge tables, queried with standard `JOIN` and `WITH RECURSIVE` (SQL:1999).
The `>` traversal sugar below compiles down to exactly that, so the data
stays queryable with plain SQL from any client that doesn't know the sugar
exists.

## Running it

```sh
cargo run -p server -- --data-file mydb.redb --bind 127.0.0.1:8801
cargo run -p cli -- --url http://127.0.0.1:8801
```

Or talk to the HTTP API directly:

```sh
curl -X POST http://127.0.0.1:8801/sql --data-binary "SELECT * FROM person;"
```

## SQL dialect

Standard SQL: `CREATE TABLE` / `DROP TABLE`, `INSERT` / `SELECT` / `UPDATE`
/ `DELETE`, `JOIN` (inner/left), `WHERE`, `GROUP BY` / `HAVING`,
`ORDER BY` / `LIMIT` / `OFFSET`, `WITH [RECURSIVE]` CTEs, `UNION [ALL]`,
aggregates (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`), scalar functions
(`UPPER`, `LOWER`, `LENGTH`, `ABS`, `ROUND`, `COALESCE`, `CONCAT`), and a
`JSON`/`JSONB` column type with `->` / `->>` operators for the
document/schemaless side.

String literals use single quotes (`'text'`); double quotes are for quoted
identifiers, per the SQL standard.

### Graph traversal sugar

A relation is just a row in a generic `edges(from_id, to_id, label)` table.
`FROM a > label > b` is sugar for joining through that table:

```sql
CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE edges (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, label TEXT);

INSERT INTO edges (from_id, to_id, label) VALUES (1, 2, 'knows');

-- one hop
SELECT p2.name FROM person p1 > knows > person p2 WHERE p1.id = 1;

-- chained fixed hops
SELECT p3.name FROM person p1 > knows > person p2 > knows > person p3 WHERE p1.id = 1;

-- variable depth: any number of 'knows' hops (handles cycles correctly)
SELECT p2.name FROM person p1 > knows* > person p2 WHERE p1.id = 1;
```

`> label* >` desugars to a synthetic `WITH RECURSIVE` transitive closure
over `edges`; everything else desugars to plain `JOIN`s. Either way, the
resulting query is ordinary SQL under the hood — inspect the desugaring in
[`crates/sql/src/parser.rs`](crates/sql/src/parser.rs) (`parse_graph_hops`,
`reachability_cte`).

Only the outgoing direction (`>`) is supported for now; incoming (`<`)
would conflict with `<` followed by a negative number (e.g. `x<-5`) if
lexed the same way, so it needs a distinct syntax and hasn't been added yet.
