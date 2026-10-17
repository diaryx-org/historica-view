//! The hub on Cloudflare.
//!
//! A repository is published to R2 as `publish.sh` lays a site out, under
//! `repos/<slug>/`, plus `history.tar`, the copy's `history/` as one archive.
//! The worker answers three kinds of request over that:
//!
//! - **Pages** — `/`, `/<slug>`, `/<slug>/tasks[/closed]`,
//!   `/<slug>/tasks/<file>` — rendered here with `dioxus-ssr` from what the
//!   repository's [`Repository`] Durable Object answers, with the data they
//!   were rendered from in the document for the browser to take over from.
//!   `/api/page?path=…` is the same data alone, which is what the browser
//!   fetches to move between pages.
//! - **Files** — everything else under `/<slug>/`, and `/<slug>.git/…` —
//!   served from R2 as stored. So `historica fetch <hub>/<slug>/offer.txt`
//!   and `git clone <hub>/<slug>.git` read the bucket through the worker and
//!   nothing in between knows what a revision is, and `/<slug>/browse/` is
//!   the static view `publish.sh` put beside the copy, for History and Files.
//! - **Pushes** — `PUT` and `DELETE /api/repos/<slug>/objects?path=…`, with
//!   a push key as the bearer token, which `push.sh` sends. Each is checked
//!   with the hub's auth worker, which says whether the key is one and
//!   whether its owner holds the repository.
//! - **Accounts** — `/api/auth/*`, passed as it came to the auth worker
//!   (`../auth/`), which runs better-auth. That worker has no address of its
//!   own: it is reached through the `AUTH` service binding, so its sign-ins
//!   and cookies are under the hub's address.
//!
//! The browser's half — the script, its wasm, the stylesheet — is the
//! worker's static assets under `/client/`, which Cloudflare serves before the
//! worker is asked, and so is `/account/`, where a reader signs in and makes
//! push keys.

#![forbid(unsafe_code)]

mod repository;

use dioxus::prelude::*;
use historica_hub_app::{App, DATA, Listed, PAGE_API, Page, ROOT, embedded};
use wasm_bindgen::JsValue;
use worker::{Env, Headers, Method, Request, RequestInit, Response, Result, event};

pub use repository::Repository;

/// The R2 binding.
pub(crate) const BUCKET: &str = "HUB";
/// The Durable Object binding.
const REPOSITORIES: &str = "REPOSITORY";
/// The service binding to the auth worker.
const AUTH: &str = "AUTH";
/// The archive of a copy's `history/` that `push.sh` uploads beside it.
pub(crate) const ARCHIVE: &str = "history.tar";

/// Where a repository's files are in the bucket.
pub(crate) fn prefix(slug: &str) -> String {
    format!("repos/{slug}/")
}

#[event(fetch)]
async fn fetch(request: Request, env: Env, _context: worker::Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    let url = request.url()?;
    let origin = url.origin().ascii_serialization();
    let path = url.path().to_owned();
    let parts: Vec<String> = path
        .trim_start_matches('/')
        .split('/')
        .map(decode)
        .collect();

    match (request.method(), parts.first().map(String::as_str)) {
        (Method::Get, Some("api")) if path == PAGE_API => {
            let Some(wanted) = url
                .query_pairs()
                .find(|(key, _)| key == "path")
                .map(|(_, value)| value.into_owned())
            else {
                return Response::error("path is wanted", 400);
            };
            let page = resolve(&env, &wanted, &origin).await?;
            Ok(Response::from_json(&page)?.with_status(status(&page)))
        }
        (_, Some("api")) if path.starts_with("/api/auth/") => {
            env.service(AUTH)?.fetch_request(request).await
        }
        (_, Some("api")) => push(request, &env, &parts).await,
        (Method::Get | Method::Head, _) => get(&env, &path, &parts, &origin).await,
        _ => Response::error("not a method the hub answers", 405),
    }
}

/// A `GET`: a page, or a file from the bucket.
async fn get(env: &Env, path: &str, parts: &[String], origin: &str) -> Result<Response> {
    let first = parts.first().map(String::as_str).unwrap_or_default();
    if let Some(slug) = first.strip_suffix(".git")
        && slug_ok(slug)
    {
        return file(
            env,
            &format!("{}{slug}.git/{}", prefix(slug), parts[1..].join("/")),
        )
        .await;
    }
    let is_page = match parts {
        [only] if only.is_empty() => true,
        [slug] | [slug, _] if slug_ok(slug) && parts.get(1).is_none_or(String::is_empty) => true,
        [slug, tasks, ..] if slug_ok(slug) && tasks == "tasks" && parts.len() <= 3 => true,
        _ => false,
    };
    if is_page {
        let page = resolve(env, path, origin).await?;
        let html = document(&page);
        let mut response = Response::from_html(html)?.with_status(status(&page));
        response.headers_mut().set("cache-control", "no-cache")?;
        return Ok(response);
    }
    let [slug, rest @ ..] = parts else {
        return Response::error("not here", 404);
    };
    if !slug_ok(slug) {
        return Response::error("not here", 404);
    }
    let key = match rest {
        [browse, inner @ ..] if browse == "browse" => {
            let inner = inner.join("/");
            let inner = if inner.is_empty() {
                "index.html".to_owned()
            } else {
                inner
            };
            format!("{}{inner}", prefix(slug))
        }
        [raw, inner @ ..] if raw == "raw" => format!("{}{slug}/{}", prefix(slug), inner.join("/")),
        _ => format!("{}{}", prefix(slug), rest.join("/")),
    };
    file(env, &key).await
}

/// One object from the bucket, as stored.
async fn file(env: &Env, key: &str) -> Result<Response> {
    if key.split('/').any(|part| part == "..") {
        return Response::error("not here", 404);
    }
    let bucket = env.bucket(BUCKET)?;
    let Some(object) = bucket.get(key).execute().await? else {
        return Response::error("not here", 404);
    };
    let Some(body) = object.body() else {
        return Response::error("not here", 404);
    };
    let bytes = body.bytes().await?;
    let mut response = Response::from_bytes(bytes)?;
    let headers = response.headers_mut();
    headers.set("content-type", content_type(key))?;
    // A published copy is meant to be read from anywhere, including the
    // static view pointed at it with `?offer=` from another host.
    headers.set("access-control-allow-origin", "*")?;
    headers.set("cache-control", "no-cache")?;
    headers.set("etag", &object.http_etag())?;
    Ok(response)
}

/// The page at `path`.
async fn resolve(env: &Env, path: &str, origin: &str) -> Result<Page> {
    let parts: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let Some(slug) = parts.first() else {
        return home(env).await;
    };
    if !slug_ok(slug) {
        return Ok(Page::Missing {
            why: format!("{path} is not a page of the hub."),
        });
    }
    let stub = env
        .durable_object(REPOSITORIES)?
        .id_from_name(slug)?
        .get_stub()?;
    let asked = format!(
        "https://repository/page?slug={}&path={}&origin={}",
        encode(slug),
        encode(path),
        encode(origin)
    );
    let mut answered = stub.fetch_with_str(&asked).await?;
    answered.json().await
}

async fn home(env: &Env) -> Result<Page> {
    let bucket = env.bucket(BUCKET)?;
    let listed = bucket
        .list()
        .prefix("repos/")
        .delimiter("/")
        .execute()
        .await?;
    let repositories = listed
        .delimited_prefixes()
        .into_iter()
        .filter_map(|prefix| {
            let slug = prefix.strip_prefix("repos/")?.strip_suffix('/')?.to_owned();
            Some(Listed { slug })
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
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="/client/style.css">
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

/// A `PUT` or `DELETE` of one object of a repository, by `push.sh`.
async fn push(mut request: Request, env: &Env, parts: &[String]) -> Result<Response> {
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    let ["api", "repos", slug, "objects"] = parts.as_slice() else {
        return Response::error("not here", 404);
    };
    let slug = slug.to_string();
    if !slug_ok(&slug) {
        return Response::error("not a name a repository can have", 400);
    }
    let Some(key) = request
        .headers()
        .get("authorization")?
        .and_then(|value| value.strip_prefix("Bearer ").map(str::to_owned))
    else {
        return Response::error("a push needs a push key: make one at /account", 401);
    };
    if let Some(refused) = authorize(env, &key, &slug).await? {
        return Ok(refused);
    }
    let url = request.url()?;
    let Some(path) = url
        .query_pairs()
        .find(|(key, _)| key == "path")
        .map(|(_, value)| value.into_owned())
    else {
        return Response::error("path is wanted", 400);
    };
    if path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Response::error("not a path within the repository", 400);
    }
    let key = format!("{}{path}", prefix(&slug));
    let bucket = env.bucket(BUCKET)?;
    match request.method() {
        Method::Put => {
            let bytes = request.bytes().await?;
            bucket.put(&key, bytes).execute().await?;
            Response::empty().map(|response| response.with_status(204))
        }
        Method::Delete => {
            bucket.delete(&key).await?;
            Response::empty().map(|response| response.with_status(204))
        }
        _ => Response::error("a push is a PUT or a DELETE", 405),
    }
}

/// Whether `key` may push to `slug`, as the auth worker answers: `None` when
/// it may, and the refusal to send back when it may not.
async fn authorize(env: &Env, key: &str, slug: &str) -> Result<Option<Response>> {
    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    let body = serde_json::json!({ "key": key, "slug": slug }).to_string();
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&body)));
    let mut answer = env
        .service(AUTH)?
        .fetch("https://auth/internal/push", Some(init))
        .await?;
    match answer.status_code() {
        204 => Ok(None),
        status @ (400 | 401 | 403) => Response::error(answer.text().await?, status).map(Some),
        status => {
            let why = answer.text().await.unwrap_or_default();
            Response::error(format!("the hub's accounts answered {status}: {why}"), 502).map(Some)
        }
    }
}

/// Whether `slug` can name a repository: a folder name, and not one of the
/// hub's own.
fn slug_ok(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 100
        && !slug.starts_with('.')
        && !matches!(
            slug,
            "account" | "api" | "client" | "favicon.ico" | "robots.txt"
        )
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
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
