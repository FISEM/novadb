# novadb

*[Français](README.md) · English*

[![CI](https://github.com/FISEM/novadb/actions/workflows/ci.yml/badge.svg)](https://github.com/FISEM/novadb/actions/workflows/ci.yml)

A relational, document and graph database in one engine, with its own query
language: **shutup**. Everything is a stream of records, and each step takes
what the step above produced.

```
person
    where age > 30
    keep following knows
    sort age down
    show name, age
```

Read it top to bottom and that is the order it runs. Indentation is the
pipeline; `|` does the same thing on one line.

## Trying it

In a browser, with nothing installed — `playground/` holds the engine
compiled to WebAssembly:

```sh
cd playground && python3 -m http.server 8080
```

As a server:

```sh
cargo run -p server -- --data-file demo.redb --bind 127.0.0.1:8801
curl -X POST http://127.0.0.1:8801/run --data-binary 'person | where age > 30'
```

At a prompt: `cargo run -p cli -- --url http://127.0.0.1:8801`

Or just `cargo test --workspace`: 232 tests, no fixtures to set up.

## The language

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
`lowest(x)`, `highest(x)`. Expressions and truthiness are Python's;
`None == None` is true.

```
# relational
define person { id: number key, name: string, age: number }

# document — no body, so every record may differ
define session
add session { device: "mobile", cart: 3 }

# graph — links live in an ordinary collection
person | where name == "alice" | keep following knows | show name
```

Deleting is the query you just read, plus a step:

```
person | where age < 18
person | where age < 18 | delete
```

And reading a field a `show` dropped is an error rather than an empty list:

```
person | show name | sort age
'age' was dropped by an earlier 'show', which kept only name.
Move this step above the show, or add age to it.
```

Full reference: [docs/language.md](docs/language.md). The reasoning behind
each choice, and what was rejected: [docs/design-notes.md](docs/design-notes.md)
and [docs/prior-art.md](docs/prior-art.md).

## Architecture

| Crate | Role |
|---|---|
| [`lang`](crates/lang/src) | shutup: lexer and parser |
| [`storage`](crates/storage/src) | key-value on [redb](https://github.com/cberner/redb), records are JSON |
| [`engine`](crates/engine/src) | runs a pipeline |
| [`server`](crates/server/src) | HTTP server (axum) |
| [`cli`](crates/cli/src) | prompt |
| [`wasm`](crates/wasm/src) | the engine in a browser |

## Status

A prototype. Single-node, no authentication, no clustering, no indexes
(every query is a full scan and `join` is a nested loop). A declared shape
is not enforced: `define person { name: string }` will not stop
`add person { nickname: "al" }`.

`define … as` and the built-in `collections` / `fields` / `queries` parse
but do not run yet. The old SQL front end is still in the tree with its
tests.

## License

MIT.
