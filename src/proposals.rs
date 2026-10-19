//! Proposals: work offered to a repository and not yet taken into it.
//!
//! A proposal is a shared bookmark under `changes/`, `changes/<id>`, whose
//! tip the accepted line does not reach. The accepted line is what the
//! repository's other shared bookmarks reach — `main`, or whatever it keeps —
//! and where it has none, what its heads reach, leaving out a head only a
//! proposal reaches. A proposal's revisions are the ones its tip reaches and
//! the accepted line does not, so a proposal taken in by a merge stops being
//! one without anyone deleting its bookmark. A private bookmark is nobody's
//! proposal, and stays out of both.
//!
//! Its title is read from a convention rather than from historica, which
//! interprets no header it does not define (its decision 0065): where the
//! proposal's first revision carries a `diaryx.change-set` header, that
//! header's value is the title, unless it only repeats the bookmark's
//! `<id>`; otherwise the title is the first line of that revision's message.
//!
//! Whoever approves it says so with a historica-minisign claim over the tip.
//! The claims are checked here, signature and all, so the page says which
//! key signed what in which role and whether the signature holds. Whose key
//! that is comes only from the trust the reader's own copy keeps in
//! `history/trust/`, which no copy carries to another; a key that trust does
//! not hold is shown as a key and nothing more.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::view::{Failure, View};

/// The prefix a proposal's bookmark is named under.
pub const PREFIX: &str = "changes/";

/// The revision header a proposal's title is read from.
pub const TITLE_HEADER: &str = "diaryx.change-set";

/// One proposal, as the History view lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proposal {
    /// The bookmark, `changes/<id>`.
    pub name: String,
    pub title: String,
    /// The revision the bookmark points at now.
    pub tip: String,
    /// The revisions the tip reaches and the accepted line does not, newest
    /// first.
    pub revisions: Vec<String>,
    /// The claims over the tip.
    pub approvals: Vec<Approval>,
}

/// One claim over a proposal's tip.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Approval {
    /// The minisign public key that made the claim.
    pub key: String,
    /// In what capacity: `reviewer`, `author`, or whatever its signer said.
    pub role: String,
    pub when: String,
    /// The one file the claim covers, by its path at the tip, for a claim
    /// over one file rather than the whole revision.
    pub file: Option<String>,
    /// Whether the signature verifies under the key the claim names.
    pub verified: bool,
    /// Who the reader's own trust takes that key to speak for, if it holds
    /// it.
    pub who: Option<String>,
}

impl View {
    /// Every open proposal, the one with the newest tip first.
    pub fn proposals(&self) -> Result<Vec<Proposal>, Failure> {
        let graph = self.graph();
        let parents: BTreeMap<&str, &[String]> = graph
            .revisions
            .iter()
            .map(|revision| (revision.id.as_str(), revision.parents.as_slice()))
            .collect();
        let shared = graph.names.iter().filter(|name| !name.private);
        let (offered, others): (Vec<_>, Vec<_>) =
            shared.partition(|name| name.name.starts_with(PREFIX));
        let offered: Vec<(&str, &str)> = offered
            .iter()
            .filter_map(|name| Some((name.name.as_str(), name.revision.as_deref()?)))
            .collect();
        if offered.is_empty() {
            return Ok(Vec::new());
        }

        let mut accepted: Vec<&str> = others
            .iter()
            .filter_map(|name| name.revision.as_deref())
            .collect();
        if accepted.is_empty() {
            let proposed = reach(&parents, offered.iter().map(|(_, tip)| *tip));
            accepted = graph
                .revisions
                .iter()
                .filter(|revision| revision.head && !proposed.contains(revision.id.as_str()))
                .map(|revision| revision.id.as_str())
                .collect();
        }
        let accepted = reach(&parents, accepted);

        let report = historica_minisign::verify::verify(self.store())
            .map_err(|error| Failure(format!("the claims in this copy cannot be read: {error}")))?;
        let position: BTreeMap<&str, usize> = graph
            .revisions
            .iter()
            .enumerate()
            .map(|(at, revision)| (revision.id.as_str(), at))
            .collect();

        let mut proposals = Vec::new();
        for (name, tip) in offered {
            if accepted.contains(tip) {
                continue;
            }
            let mut revisions: Vec<&str> = reach(&parents, [tip])
                .difference(&accepted)
                .copied()
                .collect();
            revisions.sort_by_key(|id| position.get(id).copied().unwrap_or(usize::MAX));
            let id = &name[PREFIX.len()..];
            let first = revisions.last().copied().unwrap_or(tip);
            let approvals = report
                .held()
                .iter()
                .filter(|held| held.claim.revision.to_string() == tip)
                .map(|held| Approval {
                    key: held.claim.key.as_str().to_owned(),
                    role: held.claim.role.as_str().to_owned(),
                    when: held.claim.when.as_str().to_owned(),
                    file: match &held.file {
                        historica_minisign::FileCheck::Whole => None,
                        historica_minisign::FileCheck::Found { path } => Some(path.clone()),
                        _ => held.claim.file().map(|(file, _)| file.to_string()),
                    },
                    verified: held.verified,
                    who: held.who.clone(),
                })
                .collect();
            proposals.push(Proposal {
                name: name.to_owned(),
                title: self.title(first, id),
                tip: tip.to_owned(),
                revisions: revisions.into_iter().map(str::to_owned).collect(),
                approvals,
            });
        }
        proposals.sort_by_key(|proposal| position.get(proposal.tip.as_str()).copied());
        Ok(proposals)
    }

    /// A proposal's title, from its first revision.
    fn title(&self, first: &str, id: &str) -> String {
        let document = first
            .parse()
            .ok()
            .and_then(|revision| self.store().get(&revision).ok().flatten());
        let Some(document) = document else {
            return id.to_owned();
        };
        let named = document
            .extensions
            .get(TITLE_HEADER)
            .map(|title| title.trim())
            .filter(|title| !title.is_empty() && *title != id);
        let subject = document
            .message
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty());
        named.or(subject).unwrap_or(id).to_owned()
    }
}

/// Every revision `from` reaches through parents, `from` included.
fn reach<'a>(
    parents: &BTreeMap<&'a str, &'a [String]>,
    from: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<&str> = from.into_iter().collect();
    while let Some(id) = stack.pop() {
        let Some((&id, above)) = parents.get_key_value(id) else {
            continue;
        };
        if seen.insert(id) {
            stack.extend(above.iter().map(String::as_str));
        }
    }
    seen
}
