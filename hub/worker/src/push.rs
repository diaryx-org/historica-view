//! Changes to a repository: making one, pushing to it, forking it, and its
//! members.
//!
//! ```text
//! PUT    /api/repos/<owner>/<slug>                   make it, or make it private or public
//! PUT    /api/repos/<owner>/<slug>/blobs/<digest>    the bytes of one file
//! PUT    /api/repos/<owner>/<slug>/site              the site listing
//! PUT    /api/repos/<owner>/<slug>/offer             the manifest, last
//! POST   /api/repos/<owner>/<slug>/fork              a fork, under the sender's handle
//! GET    /api/repos/<owner>/<slug>/members           who the owner added
//! PUT    /api/repos/<owner>/<slug>/members/<handle>  add one
//! DELETE /api/repos/<owner>/<slug>/members/<handle>  take one off
//! ```
//!
//! Each is sent with a push key as the bearer token, as `push.sh` sends it,
//! or with a browser's session. Nobody writes to a repository they do not
//! own: only the owner pushes to it or changes its members, and a
//! repository is made only under its maker's own handle. A contribution is a
//! fork, which is the contributor's.
//!
//! # A push
//!
//! `push.sh` sends the bytes of every file the hub has not had from it, then
//! the site listing, then the manifest. The manifest is what moves when the
//! repository is pushed to, so everything it names is there before it moves.
//!
//! # Why every push sends the bytes
//!
//! Files are kept once by digest, so the hub could skip bytes it already
//! has. It does not, and no answer here says whether it had them. A pusher
//! who could ask "do you hold this digest?" could learn whether a file is in
//! somebody else's repository, a private one included: whether a document
//! with a guessed salary in it, say, is anyone's. So:
//!
//! - **A blob is taken only with its bytes**, hashed here against the digest
//!   it is sent as, and refused where they differ. Bytes that are already
//!   in the bucket are written over with the same bytes, and the answer is
//!   the same `204` either way.
//! - **A repository holds a digest only once its bytes were pushed to it**,
//!   or it was forked with it, and its listings may name nothing else. A
//!   manifest naming a digest the repository does not hold is refused, and
//!   the refusal names only digests missing from *this* repository, which is
//!   what the pusher already sent or did not. Without this, a pusher could
//!   list a guessed digest without its bytes and read whether the hub serves
//!   it.
//!
//! What a push costs is what it changes: `push.sh` remembers what it sent,
//! and that memory is the pusher's own.
//!
//! # Letting go
//!
//! When a manifest is taken, the repository stops holding what neither of
//! its listings names, and a blob no repository holds any more is deleted.
//! That is decision 0052's withdrawal arriving at the hub: a `forget` at the
//! origin destroys the bytes here too, unless another repository, such as a
//! fork taken before the forget, still serves them. A push racing another
//! repository's withdrawal of the same bytes can lose them; pushing again
//! sends them again.

use historica_view::published::Published;
use serde::Deserialize;
use worker::{Env, Method, Request, Response, Result};

use crate::BUCKET;
use crate::account::{self, Account};
use crate::db::{self, Row};
use crate::gate::{self, Reader};
use crate::route::{Named, handle_ok, slug_ok};
use crate::site::{self, MANIFEST, SITE, Site, blob, digest_ok};

/// A request to `/api/repos/<owner>/<slug>/<rest…>`.
pub async fn answer(
    mut request: Request,
    env: &Env,
    named: &Named,
    rest: &[String],
) -> Result<Response> {
    let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
    let method = request.method();
    let Some(actor) = account::of(env, &request).await? else {
        return Response::error(
            "sign in, or send a push key as the bearer token: make one at /account",
            401,
        );
    };
    let Some(handle) = actor.username.clone() else {
        return Response::error("choose a handle at /account first", 403);
    };
    match (method, rest.as_slice()) {
        (Method::Put, []) => {
            let body = request.text().await?;
            settings(env, &actor, &handle, named, &body).await
        }
        (Method::Post, ["fork"]) => {
            let body = request.text().await?;
            fork(env, &actor, &handle, named, &body).await
        }
        (method, rest) => {
            let row = match owned(env, &actor, named).await? {
                Ok(row) => row,
                Err(refused) => return Ok(refused),
            };
            match (method, rest) {
                (Method::Put, ["blobs", digest]) => {
                    let bytes = request.bytes().await?;
                    put_blob(env, &row, digest, bytes).await
                }
                (Method::Put, ["site"]) => {
                    let text = request.text().await?;
                    put_site(env, &row, &text).await
                }
                (Method::Put, ["offer"]) => {
                    let text = request.text().await?;
                    put_offer(env, &row, &text).await
                }
                (Method::Get, ["members"]) => {
                    let members = db::members(env, row.id).await?;
                    Response::from_json(&serde_json::json!({
                        "owner": row.named.owner,
                        "members": members,
                    }))
                }
                (method @ (Method::Put | Method::Delete), ["members", member]) => {
                    member_change(env, &row, member, method == Method::Put).await
                }
                _ => Response::error("not a change the hub makes", 404),
            }
        }
    }
}

/// `named`, where `actor` owns it; otherwise the refusal: `404` where they
/// may not read it either, so a private repository stays unseen.
async fn owned(
    env: &Env,
    actor: &Account,
    named: &Named,
) -> Result<std::result::Result<Row, Response>> {
    let Some(row) = db::repository(env, named).await? else {
        return Response::error(format!("nothing is published as {}", named.spelled()), 404)
            .map(Err);
    };
    if row.owner == actor.id {
        return Ok(Ok(row));
    }
    let spelled = named.spelled();
    let readable = gate::grant(env, row, Reader::Account(actor.id.clone()))
        .await?
        .is_some();
    if readable {
        Response::error(format!("{spelled} is not yours: fork it to change it"), 403).map(Err)
    } else {
        Response::error(format!("nothing is published as {spelled}"), 404).map(Err)
    }
}

#[derive(Deserialize, Default)]
struct Settings {
    private: Option<bool>,
}

/// `PUT /api/repos/<owner>/<slug>`: make the repository, under the sender's
/// own handle, or change whether it is private.
async fn settings(
    env: &Env,
    actor: &Account,
    handle: &str,
    named: &Named,
    body: &str,
) -> Result<Response> {
    let settings: Settings = if body.trim().is_empty() {
        Settings::default()
    } else {
        match serde_json::from_str(body) {
            Ok(settings) => settings,
            Err(error) => return Response::error(format!("not settings: {error}"), 400),
        }
    };
    let row = match db::repository(env, named).await? {
        Some(_) => match owned(env, actor, named).await? {
            Ok(row) => row,
            Err(refused) => return Ok(refused),
        },
        None => {
            if named.owner != handle {
                return Response::error(
                    format!("a repository is made under your own handle, {handle}"),
                    403,
                );
            }
            db::create(
                env,
                &actor.id,
                &named.slug,
                settings.private.unwrap_or(false),
                None,
            )
            .await?;
            let row = db::repository(env, named).await?.ok_or_else(|| {
                worker::Error::RustError("the new repository is not there".into())
            })?;
            return described(&row).map(|response| response.with_status(201));
        }
    };
    if let Some(private) = settings.private
        && private != row.private
    {
        if !private {
            let chain = db::chain(env, row.id, &Reader::Anyone).await?;
            if !chain.is_some_and(|chain| chain.may_be_public()) {
                return Response::error(
                    "a fork of a private repository stays private while that repository is",
                    409,
                );
            }
        }
        db::set_private(env, row.id, private).await?;
    }
    let row = db::repository(env, named).await?.unwrap_or(row);
    described(&row)
}

fn described(row: &Row) -> Result<Response> {
    Response::from_json(&serde_json::json!({
        "owner": row.named.owner,
        "slug": row.named.slug,
        "private": row.private,
        "forkedFrom": row.forked_from,
    }))
}

/// `PUT …/blobs/<digest>`: one file's bytes, checked against the digest they
/// are sent as, and held by the repository from then on.
async fn put_blob(env: &Env, row: &Row, digest: &str, bytes: Vec<u8>) -> Result<Response> {
    if !digest_ok(digest) {
        return Response::error(
            "a blob is named by its SHA-256 digest, in lowercase hex",
            400,
        );
    }
    let hashed = historica::format::digest(&bytes).to_string();
    if hashed != digest {
        return Response::error(format!("these bytes hash to {hashed}, not {digest}"), 400);
    }
    // Held first, so a withdrawal racing this one sees a holder and leaves
    // the blob alone; then written, whether or not the bucket had it.
    db::hold(env, row.id, digest).await?;
    env.bucket(BUCKET)?
        .put(blob(digest), bytes)
        .execute()
        .await?;
    Response::empty().map(|response| response.with_status(204))
}

/// `PUT …/site`: the site listing, naming only what the repository holds.
async fn put_site(env: &Env, row: &Row, text: &str) -> Result<Response> {
    let listed = match site::listed(text) {
        Ok(listed) => listed,
        Err(why) => return Response::error(format!("{SITE}: {why}"), 400),
    };
    let digests: Vec<&str> = listed.values().map(String::as_str).collect();
    if let Some(refused) = unheld(env, row, &digests).await? {
        return Ok(refused);
    }
    env.bucket(BUCKET)?
        .put(format!("{}{SITE}", gate::prefix(row.id)), text.to_owned())
        .execute()
        .await?;
    Response::empty().map(|response| response.with_status(204))
}

/// `PUT …/offer`: the manifest, which moves the repository to what it names.
async fn put_offer(env: &Env, row: &Row, text: &str) -> Result<Response> {
    let mut published = match Published::read(text) {
        Ok(published) => published,
        Err(failure) => return Response::error(format!("{MANIFEST}: {}", failure.0), 400),
    };
    let pages = published.pages();
    let digests: Vec<&str> = pages.iter().map(|(_, digest)| digest.as_str()).collect();
    if let Some(refused) = unheld(env, row, &digests).await? {
        return Ok(refused);
    }
    let bucket = env.bucket(BUCKET)?;
    for (path, digest) in &pages {
        let bytes = read(&bucket, &blob(digest)).await?;
        if let Err(failure) = published.page(path, &bytes) {
            return Response::error(failure.0, 400);
        }
    }
    let listing = match published.listing() {
        Ok(listing) => listing,
        Err(failure) => return Response::error(failure.0, 400),
    };
    let prefix = gate::prefix(row.id);
    let Some(listed) = bucket.get(format!("{prefix}{SITE}")).execute().await? else {
        return Response::error(format!("push {SITE} before {MANIFEST}"), 409);
    };
    let listed = listed
        .body()
        .ok_or_else(|| worker::Error::RustError(format!("{SITE} has no body")))?
        .text()
        .await?;
    let site = match Site::new(&pages, &listing, &listed) {
        Ok(site) => site,
        Err(why) => return Response::error(why, 400),
    };
    let named: Vec<&str> = site.named().into_iter().collect();
    if let Some(refused) = unheld(env, row, &named).await? {
        return Ok(refused);
    }
    bucket
        .put(format!("{prefix}{MANIFEST}"), text.to_owned())
        .execute()
        .await?;
    let orphans = db::settle(env, row.id, &named).await?;
    for chunk in orphans.chunks(1000) {
        let keys: Vec<String> = chunk.iter().map(|digest| blob(digest)).collect();
        bucket.delete_multiple(keys).await?;
    }
    Response::empty().map(|response| response.with_status(204))
}

/// The refusal for a listing that names digests the repository does not
/// hold, if it does.
async fn unheld(env: &Env, row: &Row, digests: &[&str]) -> Result<Option<Response>> {
    let missing = db::missing(env, row.id, digests).await?;
    if missing.is_empty() {
        return Ok(None);
    }
    let shown: Vec<&str> = missing.iter().take(20).map(String::as_str).collect();
    Response::error(
        format!(
            "{} named here {} not been pushed to {}: send {} bytes first\n{}",
            missing.len(),
            if missing.len() == 1 {
                "file has"
            } else {
                "files have"
            },
            row.named.spelled(),
            if missing.len() == 1 { "its" } else { "their" },
            shown.join("\n"),
        ),
        409,
    )
    .map(Some)
}

#[derive(Deserialize, Default)]
struct Fork {
    slug: Option<String>,
}

/// `POST …/fork`: a fork of `named` under the sender's handle, by the same
/// slug or the one asked for. It copies the manifest and the site listing,
/// and holds what they name; no file is copied, since every one is a blob
/// both repositories serve.
async fn fork(
    env: &Env,
    actor: &Account,
    handle: &str,
    named: &Named,
    body: &str,
) -> Result<Response> {
    let asked: Fork = if body.trim().is_empty() {
        Fork::default()
    } else {
        match serde_json::from_str(body) {
            Ok(asked) => asked,
            Err(error) => return Response::error(format!("not a fork: {error}"), 400),
        }
    };
    let reader = Reader::Account(actor.id.clone());
    let missing = || Response::error(format!("nothing is published as {}", named.spelled()), 404);
    let Some(parent) = db::repository(env, named).await? else {
        return missing();
    };
    let Some(parent) = gate::grant(env, parent, reader.clone()).await? else {
        return missing();
    };
    let slug = asked.slug.unwrap_or_else(|| named.slug.clone());
    if !slug_ok(&slug) {
        return Response::error(format!("{slug} is not a name a repository can have"), 400);
    }
    let into = Named {
        owner: handle.to_owned(),
        slug,
    };
    if db::repository(env, &into).await?.is_some() {
        return Response::error(format!("you have a {} already", into.spelled()), 409);
    }
    let bucket = env.bucket(BUCKET)?;
    let mut copied = Vec::new();
    for name in [MANIFEST, SITE] {
        let key = format!("{}{name}", parent.prefix());
        let Some(object) = bucket.get(&key).execute().await? else {
            return Response::error(format!("nothing is pushed to {} yet", named.spelled()), 409);
        };
        let bytes = object
            .body()
            .ok_or_else(|| worker::Error::RustError(format!("{key} has no body")))?
            .bytes()
            .await?;
        copied.push((name, bytes));
    }
    let chain = db::chain(env, parent.id(), &reader)
        .await?
        .ok_or_else(|| worker::Error::RustError("the repository is not there".into()))?;
    let fork = chain.fork(&actor.id);
    db::create(env, &actor.id, &into.slug, fork.private, Some(parent.id())).await?;
    let row = db::repository(env, &into)
        .await?
        .ok_or_else(|| worker::Error::RustError("the fork is not there".into()))?;
    db::inherit(env, parent.id(), row.id).await?;
    // The manifest last, as a push writes it.
    for (name, bytes) in copied.into_iter().rev() {
        bucket
            .put(format!("{}{name}", gate::prefix(row.id)), bytes)
            .execute()
            .await?;
    }
    described(&row).map(|response| response.with_status(201))
}

/// `PUT` or `DELETE …/members/<handle>`.
async fn member_change(env: &Env, row: &Row, handle: &str, add: bool) -> Result<Response> {
    if !handle_ok(handle) {
        return Response::error(format!("{handle} is not a handle"), 400);
    }
    let Some(user) = db::account(env, handle).await? else {
        return Response::error(format!("no account is {handle}"), 404);
    };
    if user == row.owner {
        return if add {
            Response::empty().map(|response| response.with_status(204))
        } else {
            Response::error("the owner is always a member", 409)
        };
    }
    db::set_member(env, row.id, &user, add).await?;
    Response::empty().map(|response| response.with_status(204))
}

async fn read(bucket: &worker::Bucket, key: &str) -> Result<Vec<u8>> {
    let object = bucket.get(key).execute().await?;
    let Some(body) = object.as_ref().and_then(|object| object.body()) else {
        return Err(worker::Error::RustError(format!(
            "{key} is not in the bucket"
        )));
    };
    body.bytes().await
}
