# novadb

[![CI](https://github.com/FISEM/novadb/actions/workflows/ci.yml/badge.svg)](https://github.com/FISEM/novadb/actions/workflows/ci.yml)

**Relational, document and graph — one engine, one pipeline.** Typed
collections when you want structure, schemaless ones when you don't, and
links you can walk. Not three query styles bolted together: the same steps,
in the same order, over all three.

```
person
    where age > 30
    keep following knows
    sort age down
    show name, age
```

Read it top to bottom and that is the order it happens. There is no clause
order to memorize, nothing written first that runs last.

**The language is called shutup.** Not an accident — it is what the query
does to the ceremony. No `SELECT`, no `FROM`, no `GROUP BY … HAVING`: you
name a collection and say what to do with it, one step per line.

## Try it in your browser

`playground/` is the real engine compiled to WebAssembly. No install, no
server, nothing leaves the page.

```sh
cd playground && python3 -m http.server 8080
```

It is a static directory, so any host will serve it. See
[playground/README.md](playground/README.md).

## Try it in 30 seconds

Needs the Rust toolchain ([rustup.rs](https://rustup.rs) if you don't have
it).

```sh
git clone https://github.com/FISEM/novadb.git && cd novadb
cargo test --workspace
```

232 tests, no fixtures to set up. They are the fastest way to see what the
language does — [`crates/engine/tests`](crates/engine/tests) reads like a
tour.

## The whole language

Twelve steps, three statements, five counting words. If a word needs a
glossary, it is the wrong word.

| Step | Does |
|---|---|
| `where` | keeps matching records |
| `show` | keeps, renames, or computes fields |
| `sort` | orders — `up` by default, `down` to reverse |
| `take` / `skip` | keeps / drops the first few |
| `unique` | drops repeats |
| `join … on …` | pairs with records from elsewhere |
| `group by` | splits the stream into groups |
| `follow` / `keep following` | walks a link one step / all the way |
| `set` / `delete` | changes / removes records |

`add` puts records in, `define` states a shape or names a pipeline, `remove`
throws a collection away. Counting: `count()`, `total(x)`, `average(x)`,
`lowest(x)`, `highest(x)`.

Expressions are Python's — `and` / `or` / `not`, `in`, `is None`, chained
comparisons like `18 < age < 65`, `len(name)`, `name.upper()` — and so is
truthiness. `None` is an ordinary value: `None == None` is true, and there
is no three-valued logic to hold in your head.

Full reference: [docs/language.md](docs/language.md).

### Three shapes, one pipeline

```
# relational — typed collections
define person { id: number key, name: string, age: number }
person | where age > 30 | show name

# document — no body, so every record may differ
define session
add session { device: "mobile", cart: 3 }
add session { note: "a different shape entirely" }

# graph — links in an ordinary collection you can read
person | where name == "alice" | keep following knows | show name
```

### Deleting is the same query, plus one line

```
person | where age < 18            # look at them
person | where age < 18 | delete   # remove exactly those
```

In SQL you rewrite a `SELECT` into a `DELETE` and hope the `WHERE` survived
the edit. Here the destructive query *is* the safe one with a step added.

### It refuses rather than answering wrongly

```
person | show name | sort age
'age' was dropped by an earlier 'show', which kept only name.
Move this step above the show, or add age to it.
```

Every error names the thing, says what is wrong in a full sentence, and
names the fix. Silently returning nothing is the failure this language
exists to avoid.

## Why it is built this way

**The pipeline, not the clause list.** SQL's worst property is that reading
order is not execution order, which is why you cannot build a query
incrementally and why an alias defined in `SELECT` is unusable in `WHERE`.
Every step here takes the records the step above produced. That single
choice removes `HAVING` (it is `where` after a `show`), removes `COALESCE`
(`a or b` already returns the first thing that is there), and makes a
document collection and a graph traversal the same shape as a table scan.

**Small on purpose.** What makes SurrealQL and EdgeQL hard is not their
syntax, it is their size. The vocabulary above is the whole language, and
keeping it that short is a decision defended query by query in
[docs/design-notes.md](docs/design-notes.md), along with what was rejected
and why.

**Nothing is inferred.** Cypher decides your grouping key by looking at
which parts of a projection are not aggregates, so adding a field silently
changes what a query means. Here the grouping key is what you wrote after
`group by`, and nowhere else.

**No wire protocol.** `POST /sql` over plain HTTP, JSON in and out. Speaking
the Postgres wire protocol is a large, orthogonal project that adds nothing
to what makes this engine useful.

## Architecture

| Crate | Role |
|---|---|
| [`lang`](crates/lang/src) | shutup: lexer, parser, syntax tree |
| [`storage`](crates/storage/src) | key-value backend on [redb](https://github.com/cberner/redb); records are JSON |
| [`engine`](crates/engine/src) | runs a pipeline over storage |
| [`server`](crates/server/src) | HTTP server (axum) |
| [`cli`](crates/cli/src) | prompt over HTTP |
| [`wasm`](crates/wasm/src) | the engine in a browser |

## Status

Early and single-node: no authentication, no clustering, no secondary
indexes (a full scan backs every query, and `join` is still a nested loop).
A shape is a claim, not a constraint — `define person { name: string }` will
not stop `add person { nickname: "al" }` — which is what makes document
collections possible and what makes a typed `define` documentation rather
than a guarantee.

Not running yet, though they parse: `define … as` for naming a pipeline, and
the built-in `collections` / `fields` / `queries`. The HTTP server and the
CLI still take SQL; the browser playground is the one that speaks shutup.
The old SQL front end is still in the tree, with its own tests, until shutup
replaces it everywhere.

Treat it as a prototype to build against and break, not a production
datastore.

## License

MIT.
