# historica hub

A forge for Historica repositories on Cloudflare: a worker that renders each
repository's front page with Dioxus, a Dioxus client that takes the page over
in the browser, R2 holding every file pushed once by its digest, a Durable
Object per repository holding it open in memory, D1 saying who owns and who
may read each repository, and a second worker running better-auth for
accounts, handles and push keys.

Kept apart from the page in its own workspace so it can be taken out whole.

## How it fits together

```
 push.sh ──PUT──▶ worker ──▶ R2  blobs/sha256/<digest>   every file, once, whoever pushed it
                                 repos/<id>/offer.txt     a repository's manifest
                                 repos/<id>/site.txt      its site listing: <digest> <path>

 browser ──GET /<owner>/<slug>──▶ worker ──gate──▶ Repository (Durable Object, one per repository)
                                    │        reads the manifest, its pages and the site
                                    │        listing, opens the store from history.tar,
                                    │        every file checked against the manifest's
                                    │        digest, and keeps it open until the
                                    │        manifest moves
                                    ▼
                 dioxus-ssr renders the page, with its data as JSON beside it;
                 the client renders the same components from that data, and
                 every link after is GET /api/page?path=… rather than a load

 browser ──/api/auth/*──▶ worker ──AUTH──▶ auth worker (better-auth, D1)
 push.sh ──PUT──▶ worker ──AUTH──▶ POST /internal/key: whose push key is this?
                    └──DB──▶ D1: who owns <owner>/<slug>, who may read it,
                                 and which digests it holds
```

- **`app/`** is the pages: one set of Dioxus components, rendered by the
  worker and by the browser. The `client` feature is the browser's half —
  fetching the next page's data, the history buttons, the copy buttons.
- **`client/`** is the browser's entry point, built to
  `worker/public/client/` by `build.sh`.
- **`worker/`** is the worker: pages, files, the read gate, and the API for
  pushes, forks and members. `public/account/` is the page where a reader
  signs in, chooses a handle, and makes push keys.
- **`auth/`** is the auth worker: better-auth over D1, with email and
  password sign-in, GitHub when the hub has an OAuth app, the `username`
  plugin for handles, and the `apiKey` plugin for push keys. It sends mail
  through Cloudflare Email Sending's `send_email` binding: a link to verify
  an address, without which an account cannot sign in or push, and a link
  to reset a password. It has no
  address of its own. The hub's worker passes `/api/auth/*` to it through the
  `AUTH` service binding, so its cookies are the hub's, and asks it whose
  push key a push sends. The hub's worker binds the same D1 database for its
  own tables.

  `migrations/0001_accounts.sql` is better-auth's tables as they were first
  deployed, printed by `node scripts/schema.ts` (Node 24 or newer).
  `0002` was the spike's `/<slug>` ownership, and `0003_owners.sql` drops it
  and adds handles, repositories, members and holdings. A change to
  `src/auth.ts` that changes better-auth's tables is a new migration, written
  against what `scripts/schema.ts` prints.

## Owners

A repository is `/<owner>/<slug>`. The owner is an account's handle: 1 to 39
lowercase letters, digits and inner hyphens, unique on the hub, and not
`account`, `api` or `client`. An account chooses its handle at `/account/`,
and needs one to push or fork. The slug is unique among the owner's
repositories, so two accounts can each have a `notes`.

Nobody writes to a repository they do not own. The first push to
`/<me>/<slug>` makes it, and only its owner's keys push to it after. A
contribution is a fork, which is the contributor's own.

| | |
|---|---|
| `/<owner>/<slug>` | the front page |
| `/<owner>/<slug>/offer.txt` | the manifest: `historica fetch` it |
| `/<owner>/<slug>/<path>` | a file of the site: a page of the manifest, a file of the export |
| `/<owner>/<slug>/browse/` | the static view, for History and Files |
| `/<owner>/<slug>/raw/<path>` | a file of the exported folder, as a README links to it |
| `/<owner>/<slug>.git` | the git mirror: `git clone` it |
| `/api/repos/<owner>/<slug>` | `PUT`: make it, or make it private or public |
| `…/blobs/<digest>`, `…/site`, `…/offer` | `PUT`: a push, in that order |
| `…/fork` | `POST`: fork it, as `{"slug": …}` or by the same name |
| `…/members`, `…/members/<handle>` | `GET`, and `PUT` or `DELETE` one |

The spike's `/<slug>` addresses are gone, not moved.

## One copy of every file

R2 keeps each file once, under its SHA-256 digest, and a path is not a key in
the bucket. Each repository has two listings of its own:

- **the manifest**, `offer.txt`, and its pages, which name every file of the
  copy `historica fetch` takes by the digest it is checked against;
- **the site listing**, `site.txt`, which names the rest of what
  `publish.sh` lays out: the static view, the git mirror, the store's
  `historica.txt`, the exported folder, and `history.tar`.

`/<owner>/<slug>/<path>` is the blob that repository's own listings name at
`<path>`, and a path neither names is a `404`, whatever the bucket holds. So a
fork costs its two listings and what it changes after.

A listing may name only digests its repository *holds*: ones whose bytes
were pushed to it, hashed by the worker against the digest they were sent
as, or that it was forked with. The hub never skips bytes because it has
them already, and no answer says whether it had them, so nobody learns by
pushing whether a file is in someone else's repository. The worker's
`push.rs` has the argument in full.

When a manifest is taken, the repository stops holding what its listings no
longer name, and a blob no repository holds is deleted: a `forget` at the
origin destroys the bytes on the hub too, unless a fork still serves them.

## Forks

`POST /api/repos/<owner>/<slug>/fork`, signed in or with a push key, makes
`/<me>/<slug>`, or `/<me>/<name>` for `{"slug": "<name>"}`. It copies the two
listings and holds what they name, and records what it was forked from,
which its page shows. Nothing else is copied. The fork shares the
repository's root revision, which is what a remark names a repository by
(`../src/target.rs`), and its owner pushes to it as to any repository of
theirs.

## Private repositories

A repository is private when made with `{"private": true}`, as `push.sh`
does under `HUB_PRIVATE=1`, or when its owner makes it so later. Its members
are its owner, the accounts the owner adds, and, for a fork, the members of
what it was forked from. A fork of a private repository is private, and
cannot be made public while that repository is private.

Every read of a repository goes through one gate in the worker,
`gate::authorize_read`: pages, page data, files, `offer.txt`, the static
view and the git mirror. A reader it refuses gets a `404`, the same as for a
repository that is not there. The gate's answer is the only way to reach a
repository's Durable Object or its listings, and what a private repository
serves is `cache-control: private, no-store`, without the
`access-control-allow-origin: *` a public one has.

Every pushed file — the static view, the export, raw files, the manifest
and the git mirror — is served with `content-security-policy: sandbox
allow-scripts allow-popups` and `x-content-type-options: nosniff`. A page
someone pushed runs in an opaque origin of its own: it cannot read the hub's
cookies, and what it asks of the hub is a cross-site request, which the
session cookie (`HttpOnly`, `SameSite=Lax`, set in `auth/src/auth.ts`) is
not sent with. So a repository's HTML cannot act as its reader. The static
view reads a public repository's files as any other origin does, by
`access-control-allow-origin: *`. The hub's own pages, `/client/`,
`/account/` and `/api/auth/` are not sandboxed.

Today a reader is known by the hub's session cookie alone, so a private
repository is readable in a member's browser, and not yet by
`historica fetch` or `git clone`. `gate::reader` is where the other two ways
go: an `Authorization: Minisign <key-id> <time> <signature>` header from a
device key registered to the account, and a short-lived token as a Basic
password for git's credential helper. Both wait on historica-minisign's
request signing.

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
(cd auth && npm install && cp .dev.vars.example .dev.vars)
(cd auth && npx wrangler d1 migrations apply historica-hub-auth --local)
cd worker && npm install
npx wrangler dev -c wrangler.jsonc -c ../auth/wrangler.jsonc
```

One `wrangler dev` runs both workers, so the service binding between them
resolves, and both bind the one local D1 database. Make an account, choose a
handle and make a push key at <http://localhost:8787/account/>. `wrangler
dev` sends no mail: it writes each message to a file and logs its path, so
the link that verifies the account is in that file. Then, from another
shell:

```console
HUB_TOKEN=hub_… ./hub/push.sh ~/some/repo http://localhost:8787 ada
```

Pushing again sends only what changed since the last push, and the
repository's Durable Object reloads when the manifest moves.

## Deploying

The auth worker goes first, since the hub's worker binds to it.

In `auth/`:

1. `npx wrangler d1 create historica-hub-auth`, and put the `database_id` it
   prints into `wrangler.jsonc`.
2. Set `BETTER_AUTH_URL` in `wrangler.jsonc` to the hub's address.
3. `npx wrangler secret put BETTER_AUTH_SECRET`, a long random string. For
   GitHub sign-in, also `GITHUB_CLIENT_ID` and `GITHUB_CLIENT_SECRET` from an
   OAuth app whose callback is `<hub>/api/auth/callback/github`.
4. Enable the domain of `EMAIL_FROM` in `wrangler.jsonc` for Email Sending
   (`npx wrangler email sending enable <domain>`, and the DNS records it
   names), and turn its **Email preview** off in the dashboard, so that
   Cloudflare does not keep the messages: each one is a live link.
5. `npx wrangler d1 migrations apply historica-hub-auth --remote`.
   `0003_owners.sql` drops the spike's repositories; their files under
   `repos/<slug>/` in R2 are not read again and can be deleted.
6. `npm run deploy`.

Then in `worker/`, after `../build.sh`: `npx wrangler r2 bucket create
historica-hub` the first time, then `npm run deploy`.

To update a deployed hub, in the same order: the migrations first, since
both workers read the tables they make, then the auth worker, then the hub's
worker.

```console
./build.sh
(cd auth && npx wrangler d1 migrations list historica-hub-auth --remote)
(cd auth && npx wrangler d1 migrations apply historica-hub-auth --remote)
(cd auth && npm run deploy)
(cd worker && npm run deploy)
```

`migrations list` shows what `apply` would run; `ship ci hub` builds both
workers with `--dry-run` and uploads nothing.

## Not yet

- **Reading a private repository without a browser**: the signed requests
  and tokens above.
- **An origin of its own for what is pushed.** Pushed files are sandboxed on
  the hub's address (above); serving them from a separate domain would hold
  even where a browser ignores the sandbox, and is the stronger fix.
- **The static view of a private repository.** Sandboxed, it reads the
  repository's files from an opaque origin, which carries no session, so a
  private repository's History and Files do not load there. Its front page,
  rendered by the hub, does.
- **Repository settings** beyond private and members: handing a repository
  over, renaming it, deleting it. A handle that is changed is free for
  another account to take, and with it the old addresses.
- **A Fork button**, and pages for members and settings: today they are
  the API alone.
- **The front page lists public repositories** only, not a member's private
  ones.
- **Hydration**, once Dioxus's server half builds for a worker, or by writing
  the hydration context the client reads.
- **History and Files rendered by the hub**, rather than handed to the static
  view: the Durable Object already holds the `View` that answers them.
- **Large repositories.** The Durable Object holds the whole history, and a
  Durable Object has 128 MB; bytes the pages never read could stay in R2.
- **Contributions** — offers, issues and remarks, each in its writer's own
  chain, beside the forks they point to.
