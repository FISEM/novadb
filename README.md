# novadb

**Graph queries in plain SQL.** novadb is a small database engine that
gives you SurrealDB-style graph traversal — reachability, multi-hop
relationships, cycles handled correctly — without a proprietary query
language. If you already know `SELECT`/`JOIN`/`WHERE`, you already know
almost all of it.

```sql
CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT);
CREATE TABLE edges (id INTEGER PRIMARY KEY, from_id INTEGER, to_id INTEGER, label TEXT);

INSERT INTO edges (from_id, to_id, label) VALUES (1, 2, 'knows'), (2, 3, 'knows');

-- "who does alice know, directly or transitively?" — any depth, cycle-safe
SELECT p2.name FROM person p1 > knows* > person p2 WHERE p1.id = 1;
```

No `->edge->table` grammar to learn, no wire protocol to configure — just
SQL plus one small piece of sugar (`>`) that compiles straight down to a
`JOIN` (or a `WITH RECURSIVE` for the `*` variable-depth case). Any tool
that speaks SQL can still query the underlying tables directly.

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

## What you get

- **Standard SQL**: `CREATE TABLE`, `INSERT`/`SELECT`/`UPDATE`/`DELETE`,
  `JOIN` (inner/left), `WHERE`, `GROUP BY`/`HAVING`, `ORDER BY`/`LIMIT`/
  `OFFSET`, `WITH [RECURSIVE]` CTEs, `UNION [ALL]`, aggregates (`COUNT`,
  `SUM`, `AVG`, `MIN`, `MAX`), scalar functions (`UPPER`, `LOWER`,
  `LENGTH`, `ABS`, `ROUND`, `COALESCE`, `CONCAT`).
- **Document side**: a `JSON`/`JSONB` column type with `->`/`->>`
  operators, for schemaless fields alongside your typed columns.
- **Graph side**: the `>` traversal sugar above — single hop, chained
  fixed hops, or `*` for "any depth", all over a plain `edges` table you
  fully control.
- **Simple transport**: `POST /sql` over plain HTTP, JSON in and out. No
  driver, no wire protocol, `curl` works fine.

String literals use single quotes (`'text'`); double quotes are for
quoted identifiers, per the SQL standard — this trips up anyone coming
from languages that treat `"..."` as a string.

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

## Status

Early and single-node: no authentication, no clustering, no secondary
indexes (joins are fast, but a full table scan still backs every query),
no automated test suite yet. Treat it as a prototype to build against and
break, not a production datastore.
