//! The questions the page asks, answered natively.
//!
//! Every answer is a plain serialisable value, so the wasm layer is only a
//! spelling of these and `cargo test` can ask everything the page can.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use historica::core::{ChangeId, FileId, RevisionId};
use historica::format::Mode;
use historica::store::{Content, Name, Store};
use historica::tree::{Kind, Tree};
use serde::Serialize;
use similar::{ChangeTag, TextDiff};

use crate::memory::Memory;

/// Where the store sits in the in-memory folder.
pub const ROOT: &str = "/store";

/// Lines either side of a change that a hunk keeps.
const CONTEXT: usize = 3;

/// A store, opened on files held in memory.
pub struct View {
    store: Store<Memory>,
}

/// Every revision and name, in the order the page draws them.
#[derive(Debug, Serialize)]
pub struct Graph {
    /// Newest first, and every revision above each of its parents.
    pub revisions: Vec<RevisionSummary>,
    /// Bookmarks, by name.
    pub names: Vec<NameSummary>,
}

/// One revision, as the graph shows it.
#[derive(Debug, Serialize)]
pub struct RevisionSummary {
    pub id: String,
    pub change: String,
    pub parents: Vec<String>,
    pub supersedes: Vec<String>,
    pub author: String,
    pub when: String,
    /// Seconds since the epoch, for ordering and relative times.
    pub epoch: i64,
    pub message: String,
    pub head: bool,
    pub superseded: bool,
    pub divergent: bool,
}

/// One bookmark, and the revision it resolves to where it resolves to one.
#[derive(Debug, Serialize)]
pub struct NameSummary {
    pub name: String,
    pub kind: &'static str,
    pub target: String,
    pub private: bool,
    pub revision: Option<String>,
}

/// What one revision did to each file it touched.
#[derive(Debug, Serialize)]
pub struct RevisionDetail {
    pub id: String,
    /// What the comparison was made against; `None` for a root.
    pub against: Option<String>,
    pub files: Vec<FileChange>,
}

/// One file's change.
#[derive(Debug, Serialize)]
pub struct FileChange {
    pub file: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub kind: &'static str,
    /// `None` where either side is not lines.
    pub hunks: Option<Vec<Hunk>>,
    pub mode: Option<(&'static str, &'static str)>,
}

/// A run of lines around a change.
#[derive(Debug, Serialize)]
pub struct Hunk {
    pub old_start: usize,
    pub new_start: usize,
    pub lines: Vec<Line>,
}

/// One line of a hunk: `' '`, `'-'` or `'+'`.
#[derive(Debug, Serialize)]
pub struct Line {
    pub sign: char,
    pub text: String,
}

/// Why a question could not be answered.
#[derive(Debug)]
pub struct Failure(pub String);

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn failed(error: impl std::fmt::Display) -> Failure {
    Failure(error.to_string())
}

impl View {
    /// Open the store whose files `memory` holds under [`ROOT`].
    pub fn open(memory: Memory) -> Result<Self, Failure> {
        let store = Store::open_on(memory, Path::new(ROOT)).map_err(failed)?;
        Ok(Self { store })
    }

    /// Every revision and bookmark.
    pub fn graph(&self) -> Graph {
        let history = self.store.history();
        let heads = history.heads();
        let superseded = history.superseded();
        let divergent: BTreeSet<ChangeId> = history.divergent_changes().into_keys().collect();

        let mut summaries: BTreeMap<RevisionId, RevisionSummary> = BTreeMap::new();
        for (id, _) in self.store.revisions() {
            let Ok(Some(document)) = self.store.get(id) else {
                continue;
            };
            let when = document.when.as_str().to_string();
            let epoch = when
                .parse::<jiff::Timestamp>()
                .map(|instant| instant.as_second())
                .unwrap_or(0);
            summaries.insert(
                *id,
                RevisionSummary {
                    id: id.to_string(),
                    change: document.change.to_string(),
                    parents: document.parents.iter().map(ToString::to_string).collect(),
                    supersedes: document
                        .supersedes
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    author: document.author.clone(),
                    when,
                    epoch,
                    message: document.message.clone(),
                    head: heads.contains(id),
                    superseded: superseded.contains(id),
                    divergent: divergent.contains(&document.change),
                },
            );
        }

        let revisions = newest_first(summaries);
        let names = self
            .store
            .names()
            .iter()
            .map(|(name, bookmark)| {
                let (kind, target, revision) = match &bookmark.target {
                    Name::Revision(id) => ("revision", id.to_string(), Some(id.to_string())),
                    Name::Change(change) => (
                        "change",
                        change.to_string(),
                        current(&revisions, &change.to_string()),
                    ),
                    Name::File(file) => ("file", file.to_string(), None),
                };
                NameSummary {
                    name: name.clone(),
                    kind,
                    target,
                    private: bookmark.private,
                    revision,
                }
            })
            .collect();
        Graph { revisions, names }
    }

    /// What `id` did, against its first parent.
    ///
    /// A merge is compared against its first parent only, which shows what the
    /// merge brought in from the others. Its other sides are a later screen.
    pub fn revision(&self, id: &str) -> Result<RevisionDetail, Failure> {
        let id: RevisionId = id.parse().map_err(failed)?;
        let document = self
            .store
            .get(&id)
            .map_err(failed)?
            .ok_or_else(|| Failure(format!("no revision {id}")))?;
        let against = document.parents.iter().next().copied();

        let before = match against {
            Some(parent) => self.store.tree(&parent).map_err(failed)?,
            None => Tree::empty(),
        };
        let after = self.store.tree(&id).map_err(failed)?;

        let files: BTreeSet<FileId> = before
            .files()
            .map(|(file, _)| *file)
            .chain(after.files().map(|(file, _)| *file))
            .collect();

        let mut changes = Vec::new();
        for file in files {
            let was = before.entry(&file);
            let now = after.entry(&file);
            let old = match (against, was) {
                (Some(parent), Some(entry)) => self.text(&before, parent, &file, entry.kind)?,
                _ => Side::Absent,
            };
            let new = match now {
                Some(entry) => self.text(&after, id, &file, entry.kind)?,
                None => Side::Absent,
            };
            let moved = matches!((was, now), (Some(was), Some(now)) if was.path != now.path);
            let mode = match (was, now) {
                (Some(was), Some(now)) if was.mode != now.mode => {
                    Some((mode_name(was.mode), mode_name(now.mode)))
                }
                _ => None,
            };
            if old == new && !moved && mode.is_none() {
                continue;
            }
            let kind = match (was, now) {
                (None, Some(_)) => "added",
                (Some(_), None) => "dropped",
                _ if old != new => "edited",
                _ if moved => "moved",
                _ => "mode",
            };
            let hunks = match (&old, &new) {
                (Side::Other(_), _) | (_, Side::Other(_)) => None,
                _ => hunks(text_or_empty(&old), text_or_empty(&new)),
            };
            changes.push(FileChange {
                file: file.to_string(),
                from: was.map(|entry| entry.path.clone()),
                to: now.map(|entry| entry.path.clone()),
                kind,
                hunks,
                mode,
            });
        }
        // By where the file ends up, so a revision reads like a folder.
        changes.sort_by(|a, b| {
            let path = |change: &FileChange| change.to.clone().or_else(|| change.from.clone());
            path(a).cmp(&path(b))
        });
        Ok(RevisionDetail {
            id: id.to_string(),
            against: against.map(|parent| parent.to_string()),
            files: changes,
        })
    }

    fn text(
        &self,
        tree: &Tree,
        at: RevisionId,
        file: &FileId,
        kind: Kind,
    ) -> Result<Side, Failure> {
        if kind == Kind::Link {
            return Ok(Side::Other(None));
        }
        Ok(
            match self.store.content_in(tree, &[at], file).map_err(failed)? {
                Content::Lines(state) => Side::Lines(state.text()),
                Content::Whole(payload) => Side::Other(Some(payload)),
            },
        )
    }
}

/// One side of a file's comparison.
#[derive(Debug, PartialEq, Eq)]
enum Side {
    Absent,
    Lines(String),
    /// Bytes, by payload, or a link.
    Other(Option<RevisionId>),
}

fn text_or_empty(side: &Side) -> &str {
    match side {
        Side::Lines(text) => text,
        _ => "",
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Executable => "executable",
        _ => "plain",
    }
}

/// Hunks of a line diff, or `None` when the two are the same.
fn hunks(old: &str, new: &str) -> Option<Vec<Hunk>> {
    let diff = TextDiff::from_lines(old, new);
    let groups = diff.grouped_ops(CONTEXT);
    if groups.is_empty() {
        return None;
    }
    Some(
        groups
            .iter()
            .map(|group| {
                let first = &group[0];
                let lines = group
                    .iter()
                    .flat_map(|op| diff.iter_changes(op))
                    .map(|change| Line {
                        sign: match change.tag() {
                            ChangeTag::Equal => ' ',
                            ChangeTag::Delete => '-',
                            ChangeTag::Insert => '+',
                        },
                        text: change.value().trim_end_matches('\n').to_string(),
                    })
                    .collect();
                Hunk {
                    old_start: first.old_range().start + 1,
                    new_start: first.new_range().start + 1,
                    lines,
                }
            })
            .collect(),
    )
}

/// The revisions, newest first, with every child above its parents.
///
/// A topological order by children, taking the newest of whatever is ready
/// next, so concurrent branches interleave by time the way a reader expects.
fn newest_first(mut summaries: BTreeMap<RevisionId, RevisionSummary>) -> Vec<RevisionSummary> {
    let ids: BTreeSet<String> = summaries
        .values()
        .map(|summary| summary.id.clone())
        .collect();
    // How many children within the store each revision still waits on.
    let mut waiting: BTreeMap<String, usize> = ids.iter().map(|id| (id.clone(), 0)).collect();
    for summary in summaries.values() {
        for parent in &summary.parents {
            if let Some(count) = waiting.get_mut(parent) {
                *count += 1;
            }
        }
    }
    let by_id: BTreeMap<String, RevisionId> =
        summaries.keys().map(|id| (id.to_string(), *id)).collect();

    let mut ready: BTreeSet<(i64, String)> = waiting
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| (summaries[&by_id[id]].epoch, id.clone()))
        .collect();
    let mut ordered = Vec::with_capacity(summaries.len());
    while let Some(next) = ready.pop_last() {
        let summary = summaries
            .remove(&by_id[&next.1])
            .expect("each revision is ready once");
        for parent in &summary.parents {
            if let Some(count) = waiting.get_mut(parent) {
                *count -= 1;
                if *count == 0 {
                    ready.insert((summaries[&by_id[parent]].epoch, parent.clone()));
                }
            }
        }
        ordered.push(summary);
    }
    ordered
}

/// The revision a change bookmark stands on: the newest of its revisions
/// nothing supersedes.
fn current(revisions: &[RevisionSummary], change: &str) -> Option<String> {
    revisions
        .iter()
        .find(|summary| summary.change == change && !summary.superseded)
        .map(|summary| summary.id.clone())
}
