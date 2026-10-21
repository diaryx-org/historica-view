//! The hub on Cloudflare.
//!
//! A repository is `/<owner>/<slug>`: an account's handle, and a name among
//! that account's repositories ([`route`]). What `publish.sh` lays out for
//! one is kept in R2 by digest, once for every repository that serves it,
//! and each repository has its own manifest and site listing saying which
//! digest each of its paths is ([`site`]). Who owns a repository, who may
//! read a private one, and which digests each holds are in D1 ([`db`]).
//!
//! The worker answers four kinds of request over that:
//!
//! - **Pages** — `/` and `/<owner>/<slug>` — rendered here with
//!   `dioxus-ssr` from what the repository's [`Repository`] Durable Object
//!   answers, with the data they were rendered from in the document for the
//!   browser to take over from. `/api/page?path=…` is the same data alone,
//!   which is what the browser fetches to move between pages.
//! - **Files** — everything else under `/<owner>/<slug>/`, and
//!   `/<owner>/<slug>.git/…` — each the blob that repository's listings name
//!   at that path. So `historica fetch <hub>/<owner>/<slug>/offer.txt` and
//!   `git clone <hub>/<owner>/<slug>.git` read the bucket through the worker
//!   and nothing in between knows what a revision is, and
//!   `/<owner>/<slug>/browse/` is the static view, for History and Files:
//!   the hub's own copy of the page, from its static assets under
//!   `/client/view/`, reading the repository's files beside it.
//! - **Changes** — `/api/repos/<owner>/<slug>/…`: pushes, forks and members
//!   ([`push`]), with a push key as the bearer token or a browser's session.
//! - **Accounts** — `/api/account`, whose push key or session a request
//!   carries, for a push tool to learn its handle; and `/api/auth/*`,
//!   passed as it came to the auth worker
//!   (`../auth/`), which runs better-auth. That worker has no address of its
//!   own: it is reached through the `AUTH` service binding, so its sign-ins
//!   and cookies are under the hub's address.
//!
//! Every read of a repository, page or file, goes through one gate,
//! [`gate::authorize_read`], and a private repository is answered as absent
//! to whoever it refuses. What a private repository serves is marked
//! `private, no-store`, so nothing between the hub and its reader keeps it.
//! Every file is served sandboxed, so a page someone pushed runs in an
//! origin of its own and never as the hub.
//!
//! The browser's half — the script and its wasm — is the worker's static
//! assets under `/client/`, which Cloudflare serves before the worker is
//! asked. The stylesheet, unit's and the hub's, is `/style.css`, built into
//! the worker with the components it styles. `/account/`, where a reader
//! signs in, chooses a handle, and makes push keys, is a page like the rest.

#![forbid(unsafe_code)]

mod account;
mod db;
mod gate;
mod push;
mod repository;
mod route;
mod site;

use dioxus::prelude::*;
use historica_hub_app::{
    App, DATA, Listed, Me, PAGE_API, Page, ROOT, STYLESHEET, embedded, stylesheet,
    stylesheet_version,
};
use worker::{Env, Method, Request, Response, Result, event};

use crate::route::{Named, Route, Within};
use crate::site::{Found, MANIFEST, blob};

pub use repository::Repository;

/// The R2 binding.
pub(crate) const BUCKET: &str = "HUB";
/// The Durable Object binding.
pub(crate) const REPOSITORIES: &str = "REPOSITORY";
/// The service binding to the auth worker.
pub(crate) const AUTH: &str = "AUTH";
/// The static assets binding: the browser's half, and the view.
pub(crate) const ASSETS: &str = "ASSETS";

#[event(fetch)]
async fn fetch(request: Request, env: Env, _context: worker::Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    let url = request.url()?;
    let origin = url.origin().ascii_serialization();
    let method = request.method();
    let reading = matches!(method, Method::Get | Method::Head);

    match Route::of(url.path()) {
        Route::Auth => env.service(AUTH)?.fetch_request(request).await,
        Route::Api { repo, rest } => push::answer(request, &env, &repo, &rest).await,
        _ if !reading => Response::error("not a method the hub answers", 405),
        Route::AccountApi => match account::of(&env, &request).await? {
            Some(account) => Response::from_json(&serde_json::json!({
                "username": account.username,
            })),
            None => Response::error(
                "sign in, or send a push key as the bearer token: make one at /account",
                401,
            ),
        },
        Route::PageApi => {
            let Some(wanted) = url
                .query_pairs()
                .find(|(key, _)| key == "path")
                .map(|(_, value)| value.into_owned())
            else {
                return Response::error("path is wanted", 400);
            };
            let (page, private) = resolve(&env, &request, &wanted, &origin).await?;
            let mut response = Response::from_json(&page)?.with_status(status(&page));
            response
                .headers_mut()
                .set("cache-control", cache(private))?;
            Ok(response)
        }
        Route::Style => {
            let mut response = Response::ok(stylesheet())?;
            let headers = response.headers_mut();
            headers.set("content-type", "text/css; charset=utf-8")?;
            // Its address changes with it, so what was asked for at this
            // version is this for good.
            let current = url
                .query_pairs()
                .any(|(key, value)| key == "v" && value == stylesheet_version());
            headers.set(
                "cache-control",
                if current {
                    "public, max-age=31536000, immutable"
                } else {
                    "no-cache"
                },
            )?;
            Ok(response)
        }
        Route::File { repo, within } => file(&env, &request, &repo, &within).await,
        Route::Account => {
            let mut page = account_page(&env, &request, &origin).await?;
            // What a link from the hub's mail came back with: `?token=` from
            // a password reset, or `?error=` from a link that did not work.
            if let Page::Account { reset, problem, .. } = &mut page {
                for (key, value) in url.query_pairs() {
                    match &*key {
                        "token" => *reset = Some(value.into_owned()),
                        "error" => *problem = Some(link_problem(&value)),
                        _ => {}
                    }
                }
            }
            let mut response = Response::from_html(document(&page))?;
            response.headers_mut().set("cache-control", cache(true))?;
            Ok(response)
        }
        Route::Home | Route::Page { .. } | Route::NotHere => {
            let (page, private) = resolve(&env, &request, url.path(), &origin).await?;
            let mut response = Response::from_html(document(&page))?.with_status(status(&page));
            response
                .headers_mut()
                .set("cache-control", cache(private))?;
            Ok(response)
        }
    }
}

/// What a page may be kept as: by nobody between the hub and the reader for
/// a private repository's.
fn cache(private: bool) -> &'static str {
    if private {
        "private, no-store"
    } else {
        "no-cache"
    }
}

/// The page at `path`, and whether it is a private repository's, or the
/// reader's own account.
async fn resolve(env: &Env, request: &Request, path: &str, origin: &str) -> Result<(Page, bool)> {
    let repo = match Route::of(path) {
        Route::Home => return Ok((home(env).await?, false)),
        Route::Account => return Ok((account_page(env, request, origin).await?, true)),
        Route::Page { repo } => repo,
        _ => {
            return Ok((
                Page::Missing {
                    why: format!("{path} is not a page of the hub."),
                },
                false,
            ));
        }
    };
    let Some(granted) = gate::authorize_read(env, request, &repo).await? else {
        return Ok((absent(&repo), false));
    };
    let row = granted.row();
    let asked = format!(
        "https://repository/page?id={}&owner={}&slug={}&private={}&forked={}&origin={}",
        granted.id(),
        encode(&row.named.owner),
        encode(&row.named.slug),
        if row.private { "1" } else { "0" },
        encode(row.forked_from.as_deref().unwrap_or_default()),
        encode(origin)
    );
    let mut answered = granted.stub(env)?.fetch_with_str(&asked).await?;
    Ok((answered.json().await?, granted.private()))
}

/// What a reader is told of a repository that is not there, or that they may
/// not read: the same, so the second cannot be told from the first.
fn absent(repo: &Named) -> Page {
    Page::Missing {
        why: format!("Nothing is published as {}.", repo.spelled()),
    }
}

/// The account page for whoever `request`'s session is: their address,
/// handle and push keys, or a sign-in.
async fn account_page(env: &Env, request: &Request, origin: &str) -> Result<Page> {
    let me = match account::by_session(env, request).await? {
        Some(signed_in) => Some(Me {
            email: signed_in.email.unwrap_or_default(),
            handle: signed_in.username,
            keys: account::keys(env, request).await?,
        }),
        None => None,
    };
    Ok(Page::Account {
        me,
        origin: origin.to_owned(),
        reset: None,
        problem: None,
    })
}

/// What a mailed link's `?error=` says to the reader.
fn link_problem(error: &str) -> String {
    if error.eq_ignore_ascii_case("token_expired") {
        "That link has expired. Sign in, or ask for a new reset link, to be sent another."
    } else {
        "That link did not work. Sign in, or ask for a new reset link, to be sent another."
    }
    .to_owned()
}

async fn home(env: &Env) -> Result<Page> {
    let repositories = db::public(env)
        .await?
        .into_iter()
        .map(|named| Listed {
            owner: named.owner,
            slug: named.slug,
        })
        .collect();
    Ok(Page::Home { repositories })
}

fn status(page: &Page) -> u16 {
    match page {
        Page::Missing { .. } => 404,
        _ => 200,
    }
}

/// The whole document for `page`: the components rendered, and the data they
/// were rendered from beside them.
fn document(page: &Page) -> String {
    let mut dom = VirtualDom::new(App).with_root_context(page.clone());
    dom.rebuild_in_place();
    let body = dioxus_ssr::render(&dom);
    let title = escape(&page.title());
    let data = embedded(page);
    let version = stylesheet_version();
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="{STYLESHEET}?v={version}">
</head>
<body>
<div id="{ROOT}">{body}</div>
<script id="{DATA}" type="application/json">{data}</script>
<script type="module">import start from "/client/historica_hub_client.js"; start();</script>
</body>
</html>
"#
    )
}

/// The policy every pushed file is served under: scripts, but no origin.
const SANDBOX: &str = "sandbox allow-scripts allow-popups";

/// One file of a repository: the blob its listings name at that path, or its
/// manifest. Served sandboxed, under [`SANDBOX`].
async fn file(env: &Env, request: &Request, repo: &Named, within: &Within) -> Result<Response> {
    let Some(granted) = gate::authorize_read(env, request, repo).await? else {
        return Response::error("not here", 404);
    };
    let (kind, path) = match within {
        Within::Site(path) => ("site", path),
        Within::Raw(path) => ("raw", path),
        Within::Git(path) => ("git", path),
        Within::View(path) => return view(env, path, granted.private()).await,
    };
    let asked = format!(
        "https://repository/file?id={}&kind={kind}&path={}",
        granted.id(),
        encode(path)
    );
    let mut answered = granted.stub(env)?.fetch_with_str(&asked).await?;
    if answered.status_code() != 200 {
        return Response::error("not here", 404);
    }
    let found: Found = answered.json().await?;
    let bucket = env.bucket(BUCKET)?;
    let (key, etag) = match &found {
        Found::Manifest => (format!("{}{MANIFEST}", granted.prefix()), None),
        Found::Blob(digest) => (blob(digest), Some(format!("\"{digest}\""))),
    };
    let Some(object) = bucket.get(&key).execute().await? else {
        return Response::error("not here", 404);
    };
    let Some(body) = object.body() else {
        return Response::error("not here", 404);
    };
    let bytes = body.bytes().await?;
    let mut response = Response::from_bytes(bytes)?;
    let headers = response.headers_mut();
    headers.set("content-type", content_type(path))?;
    // Every byte here was pushed by someone, and is served from the hub's
    // own address. Sandboxed without `allow-same-origin`, a pushed page runs
    // in an opaque origin: it cannot read the hub's cookies, and what it
    // asks of the hub is a cross-site request, which a `SameSite=Lax`
    // session cookie is not sent with. So a repository's HTML cannot act as
    // its reader, on their private repositories or their keys. The static
    // view still runs, and reads a public repository's files as any other
    // origin does, by `access-control-allow-origin: *`.
    headers.set("content-security-policy", SANDBOX)?;
    headers.set("x-content-type-options", "nosniff")?;
    headers.set("etag", &etag.unwrap_or_else(|| object.http_etag()))?;
    headers.set("cache-control", cache(granted.private()))?;
    if !granted.private() {
        // A published copy is meant to be read from anywhere, including the
        // static view pointed at it with `?offer=` from another host. A
        // private one is read here, by a member, and nowhere else.
        headers.set("access-control-allow-origin", "*")?;
    }
    Ok(response)
}

/// A file of the static view, from the hub's own copy of the page, which
/// `build.sh` puts at `public/client/view/`. Under [`SANDBOX`] like a pushed
/// file, so that the page reads a repository's files as it would from any
/// other origin, and holds no session. Sandboxed, the page's origin is
/// opaque, so its own script and wasm are cross-origin to it: they are the
/// hub's code, the same for every repository, and readable from anywhere.
async fn view(env: &Env, path: &str, private: bool) -> Result<Response> {
    // The assets answer `/client/view/` for its index.html, and redirect a
    // request that names the file.
    let asked = match path {
        "index.html" => "https://assets/client/view/".to_owned(),
        path => format!("https://assets/client/view/{path}"),
    };
    let mut served = env.assets(ASSETS)?.fetch(asked, None).await?;
    if served.status_code() != 200 {
        return Response::error("not here", 404);
    }
    let bytes = served.bytes().await?;
    let mut response = Response::from_bytes(bytes)?;
    let headers = response.headers_mut();
    headers.set("content-type", content_type(path))?;
    headers.set("content-security-policy", SANDBOX)?;
    headers.set("x-content-type-options", "nosniff")?;
    headers.set("cache-control", cache(private))?;
    headers.set("access-control-allow-origin", "*")?;
    Ok(response)
}

fn content_type(key: &str) -> &'static str {
    let extension = key
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript",
        "css" => "text/css",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "txt" | "md" | "toml" | "yaml" | "yml" | "rs" | "ts" | "sh" => "text/plain; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A path segment with its percent-escapes read.
pub(crate) fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let Some(hex) = text.get(at + 1..at + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
