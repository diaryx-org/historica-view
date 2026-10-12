# historica-view

A browser view of a [Historica](https://github.com/diaryx-org/historica)
store's history. **History** is the revision graph with its heads and
bookmarks, and what each revision did to each file. **Files** is the folder as
it stood at any revision: pick a file or folder and its timeline lists every
revision that wrote it, following a file through its moves, and picking one
moves the whole folder to that moment.

The `historica` library runs in the page, compiled to WebAssembly. The page
fetches the store's files, hands them to the library in memory, and asks it
everything it shows. Nothing on the server knows what a revision is, so the
same page works against a local store or a published copy on a static host.

## Running it

You need Rust with the `wasm32-unknown-unknown` target, the `wasm-bindgen`
CLI at the version `Cargo.toml` pins, and pnpm.

```console
cd web
pnpm install
pnpm wasm
HISTORICA_STORE=~/some/repo pnpm dev
```

`HISTORICA_STORE` is a store's folder, or a folder with a store in `history/`.
The dev server lists it at `/store/files` and serves each file at
`/store/raw/<path>`, read-only. Run `pnpm wasm` again after changing the Rust.

## What is here

| Path | What it is |
|---|---|
| `src/memory.rs` | a read-only `Filesystem` over files held in memory |
| `src/view.rs` | the questions the page asks, answered natively and tested with `cargo test` |
| `src/lib.rs` | those answers, spelled as JSON for the page |
| `web/vite.config.ts` | the dev server that hands the page one local store |
| `web/src/graph.ts` | lane layout for the revision graph |
| `web/src/history.ts` | the History view |
| `web/src/merge.ts` | a merge: how its sides met, line by line |
| `web/src/files.ts` | the Files view |
| `web/src/main.ts` | loading the store, and moving between views |

## License

MIT or Apache-2.0, at your option.
