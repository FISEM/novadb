# The shutup playground

A single page that runs novadb's engine, compiled to WebAssembly. No server,
no network calls: the database lives in the tab and dies with it.

## Hosting it

Copy this directory somewhere and serve it over HTTP. Any static host will do
— GitHub Pages, Netlify, an nginx root, `python3 -m http.server`.

```sh
cd playground
python3 -m http.server 8080
```

It will not work opened as a `file://` URL: browsers refuse to load
WebAssembly modules from the filesystem. It has to be served.

Serve `.wasm` as `application/wasm`. Most hosts already do; nginx needs
`types { application/wasm wasm; }` if its mime.types is old.

## Rebuilding after a change to the engine

`pkg/` is built output, committed so the page can be hosted without a Rust
toolchain. Regenerate it after touching any crate:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
./playground/build.sh
```

## What is in here

| | |
|---|---|
| `index.html` | the whole page — no build step, no dependencies |
| `pkg/` | the engine as WebAssembly, built by `build.sh` |
| `build.sh` | rebuilds `pkg/` from the Rust crates |

Every page load seeds a fresh database with a few people, their pets, and who
knows whom, so all three shapes — relational, document and graph — have
something to work on.
