//! The hub's records in D1: repositories, their members, and what each
//! holds.
//!
//! The database is the auth worker's, which better-auth keeps its accounts
//! in; the hub's worker binds the same one as `DB` for the tables that are
//! the hub's own, which `../auth/migrations/0003_owners.sql` makes:
//!
//! - `repository`: an id, the owning account, a slug unique among that
//!   account's, whether it is private, and what it was forked from;
//! - `member`: the accounts an owner added to a repository;
//! - `held`: the digests a repository holds, which are the only ones its
//!   listings may name ([`crate::push`] says why).

use serde::Deserialize;
use wasm_bindgen::JsValue;
use worker::{D1Database, Env, Result};

use crate::gate::{Reader, Repository};
use crate::route::Named;

/// The D1 binding.
const DB: &str = "DB";

fn db(env: &Env) -> Result<D1Database> {
    env.d1(DB)
}

/// A repository's row, with its owner's handle and the name of what it was
/// forked from.
#[derive(Debug, Clone)]
pub struct Row {
    pub id: i64,
    /// The owning account's id.
    pub owner: String,
    pub named: Named,
    pub private: bool,
    /// What it was forked from, as `<owner>/<slug>`.
    pub forked_from: Option<String>,
}

#[derive(Deserialize)]
struct Raw {
    id: i64,
    owner: String,
    username: String,
    slug: String,
    private: i64,
    parent: Option<String>,
}

impl From<Raw> for Row {
    fn from(raw: Raw) -> Self {
        Row {
            id: raw.id,
            owner: raw.owner,
            named: Named {
                owner: raw.username,
                slug: raw.slug,
            },
            private: raw.private != 0,
            forked_from: raw.parent,
        }
    }
}

const ROW: &str = r#"select r."id", r."owner", u."username", r."slug", r."private",
  (select pu."username" || '/' || p."slug" from "repository" p
     join "user" pu on pu."id" = p."owner" where p."id" = r."forkedFrom") as "parent"
from "repository" r join "user" u on u."id" = r."owner""#;

/// The repository `named` names, if there is one.
pub async fn repository(env: &Env, named: &Named) -> Result<Option<Row>> {
    let raw: Option<Raw> = db(env)?
        .prepare(format!(
            r#"{ROW} where u."username" = ?1 and r."slug" = ?2"#
        ))
        .bind(&[named.owner.as_str().into(), named.slug.as_str().into()])?
        .first(None)
        .await?;
    Ok(raw.map(Row::from))
}

/// Every repository anyone may read, by name.
pub async fn public(env: &Env) -> Result<Vec<Named>> {
    #[derive(Deserialize)]
    struct Listed {
        username: String,
        slug: String,
    }
    let listed: Vec<Listed> = db(env)?
        .prepare(
            r#"select u."username", r."slug" from "repository" r
               join "user" u on u."id" = r."owner"
               where r."private" = 0 order by u."username", r."slug""#,
        )
        .all()
        .await?
        .results()?;
    Ok(listed
        .into_iter()
        .map(|listed| Named {
            owner: listed.username,
            slug: listed.slug,
        })
        .collect())
}

/// Repository `id` as the gate decides on it for `reader`: it and what it
/// was forked from, and so back, each with `reader` among its members where
/// they are one.
pub async fn chain(env: &Env, id: i64, reader: &Reader) -> Result<Option<Repository>> {
    #[derive(Deserialize)]
    struct Link {
        owner: String,
        private: i64,
        member: i64,
    }
    let reader_id = match reader {
        Reader::Account(id) => id.as_str(),
        Reader::Anyone => "",
    };
    let links: Vec<Link> = db(env)?
        .prepare(
            r#"with recursive "chain"("id", "depth") as (
                 select ?1, 0
                 union all
                 select r."forkedFrom", c."depth" + 1 from "repository" r
                   join "chain" c on r."id" = c."id"
                   where r."forkedFrom" is not null and c."depth" < 100
               )
               select r."owner", r."private",
                 exists(select 1 from "member" m
                   where m."repository" = r."id" and m."user" = ?2) as "member"
               from "chain" c join "repository" r on r."id" = c."id"
               order by c."depth""#,
        )
        .bind(&[(id as f64).into(), reader_id.into()])?
        .all()
        .await?
        .results()?;
    let mut repository = None;
    for link in links.into_iter().rev() {
        repository = Some(Repository {
            private: link.private != 0,
            owner: link.owner,
            members: if link.member != 0 {
                vec![reader_id.to_owned()]
            } else {
                Vec::new()
            },
            forked_from: repository.map(Box::new),
        });
    }
    Ok(repository)
}

/// A new repository, `slug` under the account `owner`.
pub async fn create(
    env: &Env,
    owner: &str,
    slug: &str,
    private: bool,
    forked_from: Option<i64>,
) -> Result<()> {
    let forked_from = forked_from.map_or(JsValue::NULL, |id| (id as f64).into());
    db(env)?
        .prepare(
            r#"insert into "repository" ("owner", "slug", "private", "forkedFrom", "createdAt")
               values (?1, ?2, ?3, ?4, ?5)"#,
        )
        .bind(&[
            owner.into(),
            slug.into(),
            (private as i32).into(),
            forked_from,
            now().into(),
        ])?
        .run()
        .await?;
    Ok(())
}

/// Make repository `id` private, or public.
pub async fn set_private(env: &Env, id: i64, private: bool) -> Result<()> {
    db(env)?
        .prepare(r#"update "repository" set "private" = ?2 where "id" = ?1"#)
        .bind(&[(id as f64).into(), (private as i32).into()])?
        .run()
        .await?;
    Ok(())
}

/// The id of the account whose handle is `handle`.
pub async fn account(env: &Env, handle: &str) -> Result<Option<String>> {
    db(env)?
        .prepare(r#"select "id" from "user" where "username" = ?1"#)
        .bind(&[handle.into()])?
        .first(Some("id"))
        .await
}

/// The handles of repository `id`'s added members.
pub async fn members(env: &Env, id: i64) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Member {
        username: String,
    }
    let members: Vec<Member> = db(env)?
        .prepare(
            r#"select u."username" from "member" m join "user" u on u."id" = m."user"
               where m."repository" = ?1 order by u."username""#,
        )
        .bind(&[(id as f64).into()])?
        .all()
        .await?
        .results()?;
    Ok(members.into_iter().map(|member| member.username).collect())
}

/// Add account `user` to repository `id`'s members, or take them off.
pub async fn set_member(env: &Env, id: i64, user: &str, member: bool) -> Result<()> {
    let statement = if member {
        db(env)?
            .prepare(
                r#"insert into "member" ("repository", "user", "createdAt") values (?1, ?2, ?3)
                   on conflict do nothing"#,
            )
            .bind(&[(id as f64).into(), user.into(), now().into()])?
    } else {
        db(env)?
            .prepare(r#"delete from "member" where "repository" = ?1 and "user" = ?2"#)
            .bind(&[(id as f64).into(), user.into()])?
    };
    statement.run().await?;
    Ok(())
}

/// Record that repository `id` holds `digest`: its bytes were pushed to it.
pub async fn hold(env: &Env, id: i64, digest: &str) -> Result<()> {
    db(env)?
        .prepare(
            r#"insert into "held" ("repository", "digest", "listed") values (?1, ?2, 0)
               on conflict do nothing"#,
        )
        .bind(&[(id as f64).into(), digest.into()])?
        .run()
        .await?;
    Ok(())
}

/// Which of `digests` repository `id` does not hold.
pub async fn missing(env: &Env, id: i64, digests: &[&str]) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Missing {
        value: String,
    }
    let json = serde_json::to_string(digests)?;
    let missing: Vec<Missing> = db(env)?
        .prepare(
            r#"select "value" from json_each(?2) where "value" not in
                 (select "digest" from "held" where "repository" = ?1)"#,
        )
        .bind(&[(id as f64).into(), json.into()])?
        .all()
        .await?
        .results()?;
    Ok(missing.into_iter().map(|missing| missing.value).collect())
}

/// Repository `id`'s listings now name `named` and nothing else: what it
/// holds and no longer names is let go, and what it names is marked listed,
/// which is what a fork of it takes. Answers the digests no repository
/// holds any more, whose blobs nothing can serve.
pub async fn settle(env: &Env, id: i64, named: &[&str]) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Digest {
        digest: String,
    }
    let json = serde_json::to_string(named)?;
    let db = db(env)?;
    let dropped: Vec<Digest> = db
        .prepare(
            r#"delete from "held" where "repository" = ?1
                 and "digest" not in (select "value" from json_each(?2))
               returning "digest""#,
        )
        .bind(&[(id as f64).into(), json.into()])?
        .all()
        .await?
        .results()?;
    db.prepare(r#"update "held" set "listed" = 1 where "repository" = ?1"#)
        .bind(&[(id as f64).into()])?
        .run()
        .await?;
    if dropped.is_empty() {
        return Ok(Vec::new());
    }
    let dropped: Vec<&str> = dropped
        .iter()
        .map(|digest| digest.digest.as_str())
        .collect();
    let orphans: Vec<Digest> = db
        .prepare(
            r#"select "value" as "digest" from json_each(?1) where not exists
                 (select 1 from "held" where "digest" = "value")"#,
        )
        .bind(&[serde_json::to_string(&dropped)?.into()])?
        .all()
        .await?
        .results()?;
    Ok(orphans.into_iter().map(|digest| digest.digest).collect())
}

/// Give fork `to` every digest repository `from` lists: what it serves, and
/// not what was pushed to it since and is not yet named.
pub async fn inherit(env: &Env, from: i64, to: i64) -> Result<()> {
    db(env)?
        .prepare(
            r#"insert into "held" ("repository", "digest", "listed")
               select ?2, "digest", 1 from "held" where "repository" = ?1 and "listed" = 1
               on conflict do nothing"#,
        )
        .bind(&[(from as f64).into(), (to as f64).into()])?
        .run()
        .await?;
    Ok(())
}

/// Now, as better-auth writes its own dates.
fn now() -> String {
    String::from(js_sys::Date::new_0().to_iso_string())
}
