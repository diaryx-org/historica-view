# historica hub

A forge for Historica repositories on Cloudflare: a worker that renders each
repository's front page and task tracker with Dioxus, a Dioxus client that
takes the page over in the browser, R2 holding every repository as
`../publish.sh` lays one out, and a Durable Object per repository holding it
open in memory.

A spike, kept apart from the page in its own workspace so it can be taken out
whole.

## How it fits together

```
 push.sh ──PUT──▶ worker ──▶ R2  repos/<slug>/  index.html, assets/      the static view
                                                 <slug>/                  historica export
                                                 offer.txt, offer-pages/  the paged manifest
                                                 <slug>.git/              the git mirror
                                                 history.tar              <slug>/history, archived

 browser ──GET /<slug>──▶ worker ──▶ Repository (Durable Object, one per slug)
                            │          opens the store from offer.txt + history.tar,
                            │          every file checked against the manifest's digest,
                            │          and keeps it open until the manifest moves
                            ▼
                 dioxus-ssr renders the page, with its data as JSON beside it;
                 the client renders the same components from that data, and
                 every link after is GET /api/page?path=… rather than a load
```

- **`app/`** is the pages: one set of Dioxus components, rendered by the
  worker and by the browser. The `client` feature is the browser's half —
  fetching the next page's data, the history buttons, the copy buttons.
- **`client/`** is the browser's entry point, built to
  `worker/public/client/` by `build.sh`.
- **`worker/`** is the worker: pages, the files of the bucket as stored, and
  the push API.

Everything under `/<slug>/` that is not a page is the bucket's file, so
`historica fetch <hub>/<slug>/offer.txt` and `git clone <hub>/<slug>.git`
read R2 through the worker, and `/<slug>/browse/` is the static view for
History and Files.

## Why not Dioxus fullstack

The shape is fullstack's — server rendering, the client taking over, typed
calls between them — assembled by hand, because `dioxus-server`
0.8.0-alpha.2 does not build for `wasm32-unknown-unknown`: it takes axum with
tokio's networking, and hyper-util's client, which a worker has neither of,
and its server functions run on a tokio `LocalPoolHandle`, which wants
threads. `dioxus-ssr` is runtime-agnostic, as
[DioxusLabs/dioxus#1829](https://github.com/DioxusLabs/dioxus/issues/1829)
says, and so is the client.

The client renders afresh rather than hydrating, for the same reason:
`dioxus-web`'s `hydrate` reads the serialised context fullstack's server
writes. The page data in the document is what stands in for it, and the
markup the client draws is the markup the worker sent.

## Running it

You need Rust with the `wasm32-unknown-unknown` target, the `wasm-bindgen`
CLI at the version `Cargo.lock` resolves, Node, and — to push — `historica`,
`historica-git` and `git` on `PATH` and a built page in `../web/dist`.

```console
./build.sh                                   # the client, into worker/public/client/
cd worker && npm install
cp .dev.vars.example .dev.vars               # HUB_TOKEN=dev-token
npx wrangler dev                             # builds the worker with worker-build
```

and from another shell:

```console
HUB_TOKEN=dev-token ./hub/push.sh ~/some/repo http://localhost:8787
```

Pushing again sends only what changed since the last push, and the
repository's Durable Object reloads when the manifest moves.

To deploy: `wrangler r2 bucket create historica-hub`, `wrangler secret put
HUB_TOKEN`, then `npm run deploy` in `worker/`.

## Not yet

- **Accounts.** One token can push to every repository. Diaryx's accounts are
  the obvious ones to use.
- **Hydration**, once Dioxus's server half builds for a worker, or by writing
  the hydration context the client reads.
- **History and Files rendered by the hub**, rather than handed to the static
  view: the Durable Object already holds the `View` that answers them.
- **Large repositories.** The Durable Object holds the whole history, and a
  Durable Object has 128 MB; bytes the pages never read could stay in R2.
- **Contributions** — historica-remark's `editing` remarks, as proposed
  changes.
