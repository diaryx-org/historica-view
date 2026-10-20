//! What a remark is about: a repository, or a file in it.
//!
//! historica-remark carries a remark's target as an opaque string, so the
//! forge has to choose what that string says. A host's address will not do:
//! a repository lives on several hosts, and every fork of it is another
//! repository at another address. What the repository and all its forks share
//! is the revision their history began with. So a [`Target`] names a
//! repository by that root revision's digest, which every copy computes from
//! the same bytes, and a file by the root and the file's identifier. A file's
//! identifier is minted once and outlives every path the file is given
//! (historica's decision 0008), so a remark on a file follows it through its
//! moves. The revision the writer saw is the remark's own `at`, and not part
//! of the target.
//!
//! # The spelling
//!
//! ```text
//! historica:<root>
//! historica:<root>/<file>
//! ```
//!
//! `<root>` is the root revision's digest as historica spells it, 64 lowercase
//! hexadecimal characters. `<file>` is a file identifier as historica spells
//! it, 24 characters from `k` to `z`. The two alphabets do not overlap, so
//! neither can be read as the other. Nothing else is accepted: no capitals,
//! no abbreviation, no trailing `/`, no whitespace, and no further segment.
//! One target has exactly one spelling, so two targets are the same exactly
//! when their strings are, and an index can key on the string. It is an IRI
//! in shape, which is what the Web Annotation model expects a target to be.
//!
//! # Which root
//!
//! A root is a revision with no parents. Most stores hold one. A store holds
//! several when unrelated histories were joined by a merge, or when the first
//! revision was amended, which leaves the old root beside the new one.
//!
//! A target names one root, and a store **answers to every root it holds**
//! ([`View::answers_to`]). That makes resolving a target a question of fact
//! about the store, with no choice in it: a target made before a join, or
//! before an amend, still resolves in every copy that holds the root it
//! names.
//!
//! Naming a store ([`View::target`]) does need a choice, and the rule is:
//! **the root with the lowest digest among those the current heads descend
//! from.** Current heads are the heads nothing supersedes. Two reasons:
//!
//! - Only roots the current line reaches. A copy taken from the current line
//!   holds its ancestry and nothing else, so a root only an abandoned line
//!   reaches, such as the root an amend replaced, may be missing from it. A
//!   name every current copy can resolve is the useful one.
//! - Lowest digest among those. The order has to be one every copy computes
//!   alike from the revisions alone. A revision's time is its writer's claim
//!   and two roots can share one; which head a host shows first is the
//!   host's. Digest order depends on nothing but the bytes.
//!
//! The name can change when a join brings in a root with a lower digest.
//! Targets made under the old name keep resolving, because the store still
//! holds that root. A fork taken before the join holds only its own root, and
//! does not answer to a target that names the other.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use historica::core::{FileId, RevisionId};

use crate::view::{Failure, View};

/// The scheme every target begins with.
const SCHEME: &str = "historica:";

/// A repository, or a file in it, as a remark names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Target {
    /// The root revision the repository's history begins with.
    pub root: RevisionId,
    /// The file, for a target that names one.
    pub file: Option<FileId>,
}

impl Target {
    /// The repository whose history begins at `root`.
    pub fn repository(root: RevisionId) -> Self {
        Self { root, file: None }
    }

    /// The file `file` in the repository whose history begins at `root`.
    pub fn file(root: RevisionId, file: FileId) -> Self {
        Self {
            root,
            file: Some(file),
        }
    }

    /// The repository this target is in: itself, for a repository.
    pub fn in_repository(&self) -> Self {
        Self::repository(self.root)
    }
}

impl fmt::Display for Target {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{SCHEME}{}", self.root)?;
        if let Some(file) = &self.file {
            write!(formatter, "/{file}")?;
        }
        Ok(())
    }
}

/// Why a string is not a target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidTarget(&'static str);

impl fmt::Display for InvalidTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for InvalidTarget {}

impl FromStr for Target {
    type Err = InvalidTarget;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let rest = text
            .strip_prefix(SCHEME)
            .ok_or(InvalidTarget("a target begins `historica:`"))?;
        let (root, file) = match rest.split_once('/') {
            Some((root, file)) => (root, Some(file)),
            None => (rest, None),
        };
        // historica's own parsers are strict: one length, one case, one
        // alphabet each.
        let root = root.parse().map_err(|_| {
            InvalidTarget(
                "a target's root is a revision digest, 64 lowercase hexadecimal characters",
            )
        })?;
        let file = file
            .map(|file| {
                file.parse().map_err(|_| {
                    InvalidTarget("a target's file is a file identifier, 24 characters from `k` to `z`, and nothing after it")
                })
            })
            .transpose()?;
        Ok(Self { root, file })
    }
}

impl View {
    /// Every root this store holds: each revision with no parents.
    pub fn roots(&self) -> BTreeSet<RevisionId> {
        self.store()
            .revisions()
            .filter(|(_, revision)| revision.parents.is_empty())
            .map(|(id, _)| *id)
            .collect()
    }

    /// The target naming this repository, by the rule the module documents,
    /// or `None` for a store with no revisions.
    pub fn target(&self) -> Option<Target> {
        let history = self.store().history();
        let superseded = history.superseded();
        let heads = history.heads();
        let mut current: Vec<RevisionId> = heads
            .iter()
            .filter(|head| !superseded.contains(head))
            .copied()
            .collect();
        // Every head superseded is a store only abandoned work is left in;
        // its heads are still where its history goes.
        if current.is_empty() {
            current = heads.into_iter().collect();
        }
        let mut seen = BTreeSet::new();
        let mut roots = BTreeSet::new();
        while let Some(next) = current.pop() {
            if !seen.insert(next) {
                continue;
            }
            let Some(revision) = history.get(&next) else {
                continue;
            };
            if revision.parents.is_empty() {
                roots.insert(next);
            }
            current.extend(revision.parents.iter().copied());
        }
        roots.first().copied().map(Target::repository)
    }

    /// The target naming `file` in this repository.
    ///
    /// The identifier is checked for its spelling only: a file this store has
    /// never held makes a target that no revision resolves.
    pub fn file_target(&self, file: &str) -> Result<Target, Failure> {
        let file: FileId = file.parse().map_err(|error| Failure(format!("{error}")))?;
        let root = self
            .target()
            .ok_or_else(|| Failure("an empty store names nothing".to_owned()))?
            .root;
        Ok(Target::file(root, file))
    }

    /// Whether `target` is about this store: whether it holds the root the
    /// target names.
    pub fn answers_to(&self, target: &Target) -> bool {
        self.store()
            .get(&target.root)
            .is_ok_and(|document| document.is_some_and(|document| document.parents.is_empty()))
    }

    /// Where the file `target` names is as `at` has it, or `None` where it is
    /// not in that revision's folder.
    ///
    /// Refused for a target that names no file, or that this store does not
    /// answer to.
    pub fn target_path(&self, target: &Target, at: &str) -> Result<Option<String>, Failure> {
        let Some(file) = &target.file else {
            return Err(Failure(format!("{target} names a repository, not a file")));
        };
        if !self.answers_to(target) {
            return Err(Failure(format!("this store does not answer to {target}")));
        }
        let at: RevisionId = at.parse().map_err(|error| Failure(format!("{error}")))?;
        let tree = self
            .store()
            .tree(&at)
            .map_err(|error| Failure(format!("{error}")))?;
        Ok(tree.entry(file).map(|entry| entry.path.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "2686b27369fed20b8cb8999c19396fc6f195ce36d68048848a7432ec61ffaa46";
    const FILE: &str = "kzyxwvutsrqponmlkzyxwvut";

    #[test]
    fn a_target_reads_back_as_it_was_written() {
        for spelled in [
            format!("historica:{ROOT}"),
            format!("historica:{ROOT}/{FILE}"),
        ] {
            let target: Target = spelled.parse().expect("a target");
            assert_eq!(target.to_string(), spelled);
        }
        let file: Target = format!("historica:{ROOT}/{FILE}").parse().unwrap();
        assert_eq!(file.root.to_string(), ROOT);
        assert_eq!(
            file.file.map(|file| file.to_string()).as_deref(),
            Some(FILE)
        );
        assert_eq!(
            file.in_repository().to_string(),
            format!("historica:{ROOT}")
        );
    }

    #[test]
    fn anything_but_the_one_spelling_is_refused() {
        let upper = ROOT.to_ascii_uppercase();
        let short = &ROOT[..12];
        let hex_file = &ROOT[..24];
        for refused in [
            String::new(),
            "historica:".to_owned(),
            ROOT.to_owned(),
            format!("Historica:{ROOT}"),
            format!("git:{ROOT}"),
            format!(" historica:{ROOT}"),
            format!("historica:{ROOT} "),
            format!("historica:{upper}"),
            format!("historica:{short}"),
            format!("historica:{ROOT}/"),
            format!("historica:{ROOT}/{hex_file}"),
            format!("historica:{ROOT}/{FILE}/"),
            format!("historica:{ROOT}/{FILE}/{FILE}"),
            format!("historica:{ROOT}//{FILE}"),
            format!("historica:/{ROOT}"),
            format!("historica:{ROOT}#{FILE}"),
        ] {
            assert!(refused.parse::<Target>().is_err(), "{refused:?} was taken");
        }
    }
}
