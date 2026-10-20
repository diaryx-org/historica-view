//! Who may read a repository, and the one place that asks.
//!
//! A public repository is anyone's to read. A private one is its members':
//! its owner, who always is one, the accounts the owner added, and, for a
//! fork, the members of what it was forked from. A fork of a private
//! repository is private, and stays so while anything it was forked from is,
//! since it holds that repository's history and making it public would
//! publish it.
//!
//! Every read of a repository goes through [`authorize_read`]: its pages,
//! the page data the browser fetches, its files, `offer.txt`, the static
//! view and the git mirror. It answers with a [`Granted`], and a [`Granted`]
//! is the only way to reach the repository's Durable Object or its objects in
//! the bucket, so a read that skipped the gate would not compile. A reader
//! the gate refuses is told the repository is not there, exactly as for one
//! that is not: a `404`, never a `403`, so a private repository's existence
//! does not leak to whoever guesses its name.
//!
//! # Who the reader is
//!
//! Today a reader is known by the hub's session cookie alone, which the auth
//! worker reads. A browser carries one; `historica fetch` and `git clone` do
//! not, and so read public repositories only. [`reader`] is where the two
//! other ways will go, both deferred until historica-minisign can sign a
//! request:
//!
//! - `Authorization: Minisign <key-id> <time> <signature>`, a signature over
//!   the method, host, path and time by a device key registered to an
//!   account, within a few minutes of the time it states, for
//!   `historica minisign fetch`;
//! - a short-lived token as a Basic password, minted for a signed challenge,
//!   for `git clone` through historica-git's credential helper.
//!
//! Each will name an account, as the cookie does, and the decision below is
//! the same whichever named it.

use worker::{Env, Request, Result, Stub};

use crate::account::{self, Account};
use crate::db;
use crate::route::Named;

/// Who is reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reader {
    /// Nobody the hub knows.
    Anyone,
    /// An account, by its id.
    Account(String),
}

/// A repository as the gate sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    pub private: bool,
    /// The owning account's id.
    pub owner: String,
    /// The accounts the owner added, by id. Where the hub asks about one
    /// reader, it fills in that reader alone, if they are one.
    pub members: Vec<String>,
    /// What this was forked from, if anything.
    pub forked_from: Option<Box<Repository>>,
}

impl Repository {
    /// Whether `reader` may read this repository.
    pub fn readable_by(&self, reader: &Reader) -> bool {
        !self.private
            || match reader {
                Reader::Anyone => false,
                Reader::Account(id) => self.has_member(id),
            }
    }

    /// Whether `id` is a member: the owner, an added member, or a member of
    /// what this was forked from, and so back.
    pub fn has_member(&self, id: &str) -> bool {
        self.owner == id
            || self.members.iter().any(|member| member == id)
            || self
                .forked_from
                .as_ref()
                .is_some_and(|parent| parent.has_member(id))
    }

    /// The repository a fork of this one is, made by `owner`: private when
    /// this one is, with no members of its own yet.
    pub fn fork(&self, owner: &str) -> Repository {
        Repository {
            private: self.private,
            owner: owner.to_owned(),
            members: Vec::new(),
            forked_from: Some(Box::new(self.clone())),
        }
    }

    /// Whether this may be made public: not while anything it was forked
    /// from is private.
    pub fn may_be_public(&self) -> bool {
        let mut next = self.forked_from.as_deref();
        while let Some(parent) = next {
            if parent.private {
                return false;
            }
            next = parent.forked_from.as_deref();
        }
        true
    }
}

/// A repository the gate let a reader through to.
///
/// Made only here, by [`grant`]. Holding one is what reaching the
/// repository's Durable Object and its objects takes, so nothing reads a
/// repository the gate did not open.
#[derive(Debug, Clone)]
pub struct Granted {
    row: db::Row,
}

impl Granted {
    /// The repository's id, which its objects and its Durable Object are
    /// named by.
    pub fn id(&self) -> i64 {
        self.row.id
    }

    pub fn row(&self) -> &db::Row {
        &self.row
    }

    /// Whether what is read must not be kept by anything between the hub
    /// and the reader.
    pub fn private(&self) -> bool {
        self.row.private
    }

    /// The repository's Durable Object.
    pub fn stub(&self, env: &Env) -> Result<Stub> {
        env.durable_object(crate::REPOSITORIES)?
            .id_from_name(&format!("repository:{}", self.row.id))?
            .get_stub()
    }

    /// Where this repository's own objects are in the bucket: its manifest
    /// and its site listing. Its files are blobs, kept once for every
    /// repository.
    pub fn prefix(&self) -> String {
        prefix(self.row.id)
    }
}

/// Where repository `id`'s own objects are in the bucket. By id, not by
/// name, so a renamed account keeps its repositories, and ids are never
/// reused, so a repository made later cannot find an older one's objects.
pub(crate) fn prefix(id: i64) -> String {
    format!("repos/{id}/")
}

/// The one gate every read of a repository goes through: `None` where the
/// reader may not read `named`, or there is nothing by that name, which the
/// caller answers alike, with a `404`.
pub async fn authorize_read(
    env: &Env,
    request: &Request,
    named: &Named,
) -> Result<Option<Granted>> {
    let Some(row) = db::repository(env, named).await? else {
        return Ok(None);
    };
    // A public repository asks nobody who the reader is.
    let reader = if row.private {
        reader(env, request).await?
    } else {
        Reader::Anyone
    };
    grant(env, row, reader).await
}

/// Whether `reader` may read `row`, and the [`Granted`] if so.
pub async fn grant(env: &Env, row: db::Row, reader: Reader) -> Result<Option<Granted>> {
    if row.private {
        let repository = db::chain(env, row.id, &reader).await?;
        if !repository.is_some_and(|repository| repository.readable_by(&reader)) {
            return Ok(None);
        }
    }
    Ok(Some(Granted { row }))
}

/// Who sent `request`.
pub async fn reader(env: &Env, request: &Request) -> Result<Reader> {
    // Here go the two ways a reader without a browser will be known, ahead
    // of the cookie: an `Authorization: Minisign …` header checked against
    // the device keys registered to an account, and a short-lived token
    // taken as a Basic password. Neither is checked yet; until they are, a
    // request carrying one is read as nobody's.
    Ok(match account::by_session(env, request).await? {
        Some(Account { id, .. }) => Reader::Account(id),
        None => Reader::Anyone,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn private(owner: &str, members: &[&str]) -> Repository {
        Repository {
            private: true,
            owner: owner.into(),
            members: members.iter().map(|member| (*member).into()).collect(),
            forked_from: None,
        }
    }

    fn account(id: &str) -> Reader {
        Reader::Account(id.into())
    }

    #[test]
    fn a_public_repository_is_anyones() {
        let public = Repository {
            private: false,
            ..private("ada", &[])
        };
        assert!(public.readable_by(&Reader::Anyone));
        assert!(public.readable_by(&account("grace")));
    }

    #[test]
    fn a_private_repository_is_its_members_alone() {
        let repository = private("ada", &["grace"]);
        assert!(
            repository.readable_by(&account("ada")),
            "the owner is a member"
        );
        assert!(repository.readable_by(&account("grace")));
        assert!(!repository.readable_by(&account("mallory")));
        assert!(!repository.readable_by(&Reader::Anyone));
    }

    #[test]
    fn a_fork_of_a_private_repository_is_private() {
        let parent = private("ada", &["grace"]);
        let fork = parent.fork("grace");
        assert!(fork.private);
        assert!(
            !fork.may_be_public(),
            "not while what it came from is private"
        );
        // Its owner and the parent's members read it; nobody else does.
        assert!(fork.readable_by(&account("grace")));
        assert!(fork.readable_by(&account("ada")));
        assert!(!fork.readable_by(&account("mallory")));
        assert!(!fork.readable_by(&Reader::Anyone));
        // A member the fork's owner adds reads the fork, not the parent.
        let mut fork = fork;
        fork.members.push("lin".into());
        assert!(fork.readable_by(&account("lin")));
        assert!(!parent.readable_by(&account("lin")));
        // And a fork of that fork is private to all of them.
        let again = fork.fork("lin");
        assert!(again.private);
        assert!(again.readable_by(&account("ada")));
        assert!(!again.readable_by(&account("mallory")));
    }

    #[test]
    fn a_fork_of_a_public_repository_is_public_and_may_stay_so() {
        let parent = Repository {
            private: false,
            ..private("ada", &[])
        };
        let fork = parent.fork("grace");
        assert!(!fork.private);
        assert!(fork.may_be_public());
        assert!(fork.readable_by(&Reader::Anyone));
    }
}
