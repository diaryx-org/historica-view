# historica-view

A browser view of a [Historica](https://github.com/diaryx-org/historica)
repository, and the page a published one is read through.

- **Overview** is the README at any revision, with the commands that take a
  copy: `historica fetch` from the published manifest, and `git clone` from
  the git mirror published beside it.
- **Files** is the folder as it stood at any revision: pick a file or folder
  and its timeline lists every revision that wrote it, following a file
  through its moves, and picking one moves the whole folder to that moment.
- **History** is the revision graph with its heads and bookmarks, and what
  each revision did to each file, with the open proposals above it. A
  proposal is a shared bookmark `changes/<id>` that the accepted line does not
  reach: what the other shared bookmarks reach, or the heads where there are
  none. Its page lists its revisions and every
  [historica-minisign](https://github.com/diaryx-org/historica-minisign) claim
  over its tip, each signature checked in the page. A key is named only where
  the trust kept in the reader's own copy, `history/trust/`, names it; a
  published copy's trust is never read.

A repository that keeps [prov](https://github.com/diaryx-org/prov)'s
documents, a task tracker among them, shows them in Files as it shows any
other file. The view reads nothing as a tracker.

A proposal's title is a convention, not anything historica defines: the
`diaryx.change-set` header on its first revision where that says more than
the bookmark's `<id>`, and otherwise the first line of that revision's
message.

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

## Publishing it

```console
pnpm --dir web build
./publish.sh ~/some/repo ~/some/site
```

writes a folder any static host can serve: the page, the repository as
`historica export` writes it (decision 0052's published copy, never the live
store), the paged manifest `historica offer --into` keeps beside it, and a bare
git mirror written by
[historica-git](https://github.com/diaryx-org/historica-git) with `git
update-server-info` run in it, so `git clone <url>/<name>.git` works over plain
HTTP. Run it again after recording and every part is brought up to date
incrementally: the export withdraws what is gone, the manifest gains one page,
and the mirror moves only the refs it made. It needs `historica`,
`historica-git` and `git` on `PATH`.

The page reads the copy beside it, `offer.txt`, the way `historica fetch` does:
each page of the manifest is hashed against the digest the manifest names it
by, and each file against the digest its line names, before the store sees it.
`?offer=<url>` points the same page at any other published copy, where that
host allows the page to fetch from it.

## The hub

[`hub/`](hub/README.md) is the same repository served from Cloudflare: a
worker that renders each repository's front page with Dioxus, R2
holding what `publish.sh` makes, and a Durable Object per repository holding
it open. It is a spike, in a workspace of its own.

## What is here

| Path | What it is |
|---|---|
| `src/memory.rs` | a read-only `Filesystem` over files held in memory |
| `src/view.rs` | the questions the page asks, answered natively and tested with `cargo test` |
| `src/published.rs` | a published copy's manifest, whole or paged, and the files it lists |
| `src/workspace.rs` | the README at a revision |
| `src/proposals.rs` | the open proposals, and the claims over their tips |
| `src/document.rs` | frontmatter, links between documents, and Markdown rendered to HTML |
| `src/lib.rs` | those answers, spelled as JSON for the page |
| `web/vite.config.ts` | the dev server that hands the page one local store |
| `web/src/graph.ts` | lane layout for the revision graph |
| `web/src/history.ts` | the History view |
| `web/src/merge.ts` | a merge: how its sides met, line by line |
| `web/src/files.ts` | the Files view |
| `web/src/load.ts` | where the store comes from: `?offer=`, `offer.txt`, or the dev server |
| `web/src/overview.ts` | the Overview: the README and how to take a copy |
| `web/src/markdown.ts` | a rendered document sanitised, with its links and images read inside the store |
| `web/src/main.ts` | moving between views |
| `publish.sh` | a repository published as a site |

## License

MIT or Apache-2.0, at your option.
