#!/usr/bin/env bash
# Rebuilds the playground's WebAssembly bundle from the Rust engine.
#
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli
#
# Then run this from anywhere in the repo.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build -p novadb-wasm --target wasm32-unknown-unknown --release
wasm-bindgen --target web \
    --out-dir playground/pkg \
    target/wasm32-unknown-unknown/release/novadb_wasm.wasm

echo "playground/pkg is up to date — serve the playground directory over HTTP."
