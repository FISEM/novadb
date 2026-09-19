# novadb

[![CI](https://github.com/FISEM/novadb/actions/workflows/ci.yml/badge.svg)](https://github.com/FISEM/novadb/actions/workflows/ci.yml)

**Relational, document, and graph — one engine, plain SQL.** novadb gives
you the range SurrealDB is known for: typed tables when you want structure,
MongoDB-style schemaless collections when you don't, and graph traversal for
relationships — without a proprietary query language. If you already know
`SELECT`/`JOIN`/`WHERE`, you already know almost all of it.

```sql
-- relational: typed columns, like always
CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);

-- document: no columns declared, any shape per row — a real collection
CREATE TABLE session;
INSERT INTO session (device, cart) VALUES ('mobile', '{"items": 3}');

-- graph: relationships as data, traversed with sugar over a plain edges table
CREATE TABLE edges (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, label TEXT);
INSERT INTO edges (from_id, to_id, label) VALUES (1, 2, 'knows'), (2, 3, 'knows');

-- "who does alice know, directly or transitively?" — any depth, cycle-safe
SELECT p2.name FROM person p1 > knows* > person p2 WHERE p1.id = 1;
```

No `->edge->table` grammar, no `db.collection.insertOne`, no wire protocol to
configure — just SQL, plus one small piece of sugar (`>`) that compiles
straight down to a `JOIN` (or a `WITH RECURSIVE` for the `*` variable-depth
case). Any tool that speaks SQL can still query every table directly, schemaless
ones included.

**[Try it in your browser →](https://claude.ai/artifact/7G6g4qj5qoovtQob5gvhv9)**
No install, no server — the real engine compiled to WebAssembly, running
entirely client-side.

## Try it in 30 seconds

Needs the Rust toolchain ([rustup.rs](https://rustup.rs) if you don't have
it — `cargo --version` to check).

```sh
git clone https://github.com/FISEM/novadb.git && cd novadb
cargo run -p server -- --data-file demo.redb --bind 127.0.0.1:8801 &
curl -X POST http://127.0.0.1:8801/sql --data-binary "
  CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
  INSERT INTO person (id, name) VALUES (1, 'alice');
  SELECT * FROM person;
"
```

Or use the bundled REPL instead of curl:

```sh
cargo run -p cli -- --url http://127.0.0.1:8801
```

To build the browser playground yourself instead of using the hosted one
above: `crates/wasm` compiles the engine to WebAssembly via `wasm-bindgen`
(`cargo build -p novadb-wasm --target wasm32-unknown-unknown --release`,
then `wasm-bindgen --target web --out-dir crates/wasm/pkg <wasm file>`),
and `crates/wasm/playground.html` is the static page that loads it — open
it from that directory once `pkg/` exists.

## What you get

- **Standard SQL**: `CREATE TABLE`, `INSERT`/`SELECT`/`UPDATE`/`DELETE`,
  `JOIN` (inner/left), `WHERE`, `GROUP BY`/`HAVING`, `ORDER BY`/`LIMIT`/
  `OFFSET`, `WITH [RECURSIVE]` CTEs, `UNION [ALL]`, aggregates (`COUNT`,
  `SUM`, `AVG`, `MIN`, `MAX`), scalar functions (`UPPER`, `LOWER`,
  `LENGTH`, `ABS`, `ROUND`, `COALESCE`, `CONCAT`).
- **Document collections**: `CREATE TABLE foo;` with no column list makes
  a schemaless table — every row can carry different fields, like a
  MongoDB collection — plus a `JSON`/`JSONB` column type with `->`/`->>`
  operators for nested fields inside an otherwise typed table.
- **Graph traversal**: the `>` sugar above — single hop, chained fixed
  hops, or `*` for "any depth", all over a plain `edges` table you fully
  control.
- **Simple transport**: `POST /sql` over plain HTTP, JSON in and out. No
  driver, no wire protocol, `curl` works fine.

String literals use single quotes (`'text'`); double quotes are for
quoted identifiers, per the SQL standard — this trips up anyone coming
from languages that treat `"..."` as a string.

### More document examples

```sql
CREATE TABLE session;

INSERT INTO session (user_id, device) VALUES (1, 'mobile');
INSERT INTO session (payload, ts) VALUES ('{"foo": "bar"}', 12345);

SELECT * FROM session;
-- both rows come back, each keeping only the fields it was given
```

`INSERT` never validates column names against a declared schema — that's
true for every table, not just schemaless ones — so this works precisely
because nothing stops you from inserting fields nobody declared. A typed
`CREATE TABLE` is a convention your queries can rely on, not a constraint
the engine enforces (yet).

### More graph examples

```sql
-- chained fixed hops
SELECT p3.name FROM person p1 > knows > person p2 > knows > person p3 WHERE p1.id = 1;

-- mixing a bounded hop with a variable-depth one
SELECT p3.name FROM person p1 > knows* > person p2 > follows > person p3 WHERE p1.id = 1;
```

Only the outgoing direction (`>`) exists today; incoming (`<`) is planned
but needs a distinct syntax to avoid clashing with `<` followed by a
negative number (`x<-5`).

## Why it's built this way

**No custom query language.** The alternative to `>` traversal sugar was
inventing a full SurrealQL-style grammar. That throws away the one thing
SQL gives you for free: every tool, ORM, and SQL-literate developer
already knows how to use it. The graph sugar is a thin desugaring layer
in the parser ([`parser.rs`](crates/sql/src/parser.rs),
`parse_graph_hops` / `reachability_cte`) — turn it into a `JOIN` /
`WITH RECURSIVE`, done, nothing new to teach.

**No pg-wire protocol.** Speaking the real Postgres wire protocol (auth,
extended query protocol, type OIDs) is a large, orthogonal project that
doesn't add to what makes this engine useful. Plain HTTP/JSON gets you
the same result — send text, get JSON — for a fraction of the effort. A
pg-wire adapter is a plausible add-on later, once the core engine earns
it.

## Architecture

| Crate | Role |
|---|---|
| [`sql`](crates/sql/src) | Lexer, parser, AST — including the `>` graph sugar |
| [`storage`](crates/storage/src) | Key-value backend on [redb](https://github.com/cberner/redb); rows are JSON documents |
| [`engine`](crates/engine/src) | Executes statements: joins (hash-join fast path for equi-joins), aggregates, recursive CTEs |
| [`server`](crates/server/src) | HTTP server (axum) exposing `POST /sql` |
| [`cli`](crates/cli/src) | REPL client over HTTP |

## Tests

`cargo test --workspace` runs the suite: 101 tests, no fixtures to set up.
Every engine test drives a fresh in-memory database through the same
text-in/JSON-out path the HTTP server uses, so what the tests exercise is
what a caller gets.

| Suite | Covers |
|---|---|
| [`sql/tests/parser.rs`](crates/sql/tests/parser.rs) | Grammar and lexing: quoting rules, precedence, graph-sugar desugaring, parse errors |
| [`engine/tests/relational.rs`](crates/engine/tests/relational.rs) | CRUD, `WHERE`, joins, aggregates, `GROUP BY`/`HAVING`, CTEs, `UNION`, scalar functions |
| [`engine/tests/document.rs`](crates/engine/tests/document.rs) | Schemaless tables: mixed row shapes, absent fields, undeclared columns |
| [`engine/tests/graph.rs`](crates/engine/tests/graph.rs) | `>` traversal: single, chained, and variable-depth hops, including cycles |
| [`engine/tests/ordering.rs`](crates/engine/tests/ordering.rs) | `ORDER BY` resolution against dropped and renamed columns |
| [`engine/tests/persistence.rs`](crates/engine/tests/persistence.rs) | What a file-backed database still holds after a reopen |

## Status

Early and single-node: no authentication, no clustering, no secondary
indexes (joins are fast, but a full table scan still backs every query).
Schema declarations aren't enforced — that's
what makes document collections possible, but it also means a typed
`CREATE TABLE` today is documentation, not a guarantee; nothing stops a
mistyped `INSERT` from smuggling in a field that doesn't belong. Treat it
as a prototype to build against and break, not a production datastore.

`cargo fmt` and `cargo clippy` are not clean yet, so CI runs the build and
the test suite only.
