#!/bin/sh
# Build the hub's browser half into worker/public/client/, where the worker
# serves it from. The worker itself is built by `wrangler dev` and `wrangler
# deploy`, through the build command in worker/wrangler.jsonc.
#
# Needs the wasm32-unknown-unknown target and the wasm-bindgen CLI at the
# version Cargo.lock resolves.

set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

cargo build --release --target wasm32-unknown-unknown -p historica-hub-client
want=$(sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"$/\1/p;}' Cargo.lock)
have=$(wasm-bindgen --version 2>/dev/null | cut -d' ' -f2 || true)
[ "$want" = "$have" ] || {
  echo "build: Cargo.lock has wasm-bindgen $want, the CLI is ${have:-missing}" >&2
  echo "  cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
}
wasm-bindgen --target web --no-typescript --out-dir worker/public/client \
  target/wasm32-unknown-unknown/release/historica_hub_client.wasm
echo "built worker/public/client/historica_hub_client.{js,wasm}"
