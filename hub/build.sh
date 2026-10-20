#!/bin/sh
# Build the hub's browser half into worker/public/client/, where the worker
# serves it from, and copy the static view, ../web/dist, to
# worker/public/client/view/, which the worker serves as every repository's
# /browse/. The worker itself is built by `wrangler dev` and `wrangler
# deploy`, through the build command in worker/wrangler.jsonc.
#
# Needs the wasm32-unknown-unknown target, the wasm-bindgen CLI at the
# version Cargo.lock resolves, and a built page (`pnpm --dir web build`).

set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here"
[ -f ../web/dist/index.html ] || {
  echo "build: no built page; run \`pnpm --dir web build\` first" >&2
  exit 1
}

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
rm -rf worker/public/client/view
cp -R ../web/dist worker/public/client/view
echo "built worker/public/client/historica_hub_client.{js,wasm}, and copied the view"
