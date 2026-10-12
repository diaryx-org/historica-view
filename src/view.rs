//! The questions the page asks, answered natively.
//!
//! Every answer is a plain serialisable value, so the wasm layer is only a
//! spelling of these and `cargo test` can ask everything the page can.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use historica::core::{ChangeId, FileId, RevisionId};
use historica::format::{Mode, RevisionDocument};
use historica::merge::Contest;
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
    /// Every revision in [`View::graph`]'s order, which every history keeps.
    order: Vec<RevisionId>,
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

/// How a merge's parents came together, file by file.
#[derive(Debug, Serialize)]
pub struct MergeDetail {
    pub id: String,
    pub parents: Vec<String>,
    /// Only the files the parents both changed, or the merge changed itself.
    pub files: Vec<MergeFile>,
}

/// One file the parents met in.
#[derive(Debug, Serialize)]
pub struct MergeFile {
    pub file: String,
    pub path: String,
    /// The parents' automatic merge, line by line, with who wrote each.
    pub lines: Vec<MergeLine>,
    /// Where concurrent work met, as indexes into `lines`.
    pub contests: Vec<ContestSummary>,
    /// What the merge recorded on top of the automatic merge; `None` where it
    /// took the automatic merge as it was.
    pub hand: Option<Vec<Hunk>>,
}

/// One line of an automatic merge.
#[derive(Debug, Serialize)]
pub struct MergeLine {
    pub text: String,
    /// The revision that wrote it.
    pub origin: String,
    /// The parent whose side alone wrote it, by index, or `None` for a line
    /// every parent already had.
    pub side: Option<usize>,
}

/// One region where concurrent revisions met.
#[derive(Debug, Serialize)]
pub struct ContestSummary {
    pub at: usize,
    /// Zero for a removal that met writing beside it.
    pub len: usize,
    /// `insertion`, `deletion` or `terminator`.
    pub kind: &'static str,
    pub revisions: Vec<String>,
}

/// One file in a folder.
#[derive(Debug, Serialize)]
pub struct TreeFile {
    pub file: String,
    pub path: String,
    /// `lines`, `bytes` or `link`.
    pub kind: &'static str,
    pub size: Option<u64>,
}

/// A revision that names a file, and what it did to it.
#[derive(Debug, Serialize)]
pub struct Version {
    pub revision: String,
    /// `added`, `edited`, `moved`, `mode` or `dropped`.
    pub kind: &'static str,
    /// Where the file was after it, or before it for a drop.
    pub path: Option<String>,
}

/// A revision that names files in a folder.
#[derive(Debug, Serialize)]
pub struct FolderVersion {
    pub revision: String,
    pub files: usize,
}

/// One file as a revision has it.
#[derive(Debug, Serialize)]
pub struct FileAt {
    pub file: String,
    pub path: String,
    pub kind: &'static str,
    /// The content, for a file of lines.
    pub text: Option<String>,
    pub size: Option<u64>,
    /// Where a link points.
    pub target: Option<String>,
    /// Whether this copy holds a file of bytes' bytes.
    pub held: bool,
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
        let mut view = Self {
            store,
            order: Vec::new(),
        };
        view.order = view
            .graph()
            .revisions
            .iter()
            .filter_map(|summary| summary.id.parse().ok())
            .collect();
        Ok(view)
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

    /// What `id` did, against `against` or else its first parent.
    ///
    /// Against one parent of a merge, this is what the merge brought in from
    /// the others; [`View::merge`] is how the sides met.
    pub fn revision(&self, id: &str, against: Option<&str>) -> Result<RevisionDetail, Failure> {
        let id: RevisionId = id.parse().map_err(failed)?;
        let document = self.document(&id)?;
        let against = match against {
            Some(against) => Some(against.parse::<RevisionId>().map_err(failed)?),
            None => document.parents.iter().next().copied(),
        };

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

    /// How the parents of the merge `id` came together.
    ///
    /// For each file of lines the parents disagree about, the merge the
    /// library makes of them — every line labelled with the revision that
    /// wrote it and the parent whose side that was — and the regions where
    /// their work met. A file only one side changed is left out: the merge
    /// took that side whole, and the comparison against the other parent
    /// shows it.
    pub fn merge(&self, id: &str) -> Result<MergeDetail, Failure> {
        let id: RevisionId = id.parse().map_err(failed)?;
        let parents: Vec<RevisionId> = self.document(&id)?.parents.iter().copied().collect();
        let sides: Vec<BTreeSet<RevisionId>> = parents
            .iter()
            .map(|parent| self.ancestors(*parent))
            .collect();
        let trees = parents
            .iter()
            .map(|parent| self.store.tree(parent).map_err(failed))
            .collect::<Result<Vec<Tree>, Failure>>()?;
        let after = self.store.tree(&id).map_err(failed)?;

        let mut files = Vec::new();
        for (file, entry) in after.entries() {
            if entry.kind != Kind::Lines {
                continue;
            }
            // A side that never had the file did not disagree about it.
            let mut texts = Vec::new();
            for (parent, tree) in parents.iter().zip(&trees) {
                match tree.entry(file) {
                    Some(held) if held.kind == Kind::Lines => {
                        texts.push(self.store.content(parent, file).map_err(failed)?.text());
                    }
                    _ => break,
                }
            }
            if texts.len() < parents.len() || texts.iter().all(|text| *text == texts[0]) {
                continue;
            }
            let merged = self
                .store
                .merged_content_of(&parents, file)
                .map_err(failed)?;
            let automatic = merged.state.text();
            let recorded = self.store.content(&id, file).map_err(failed)?.text();
            if merged.contested.is_empty() && recorded == automatic && texts.contains(&automatic) {
                continue;
            }
            let lines = merged
                .state
                .items()
                .iter()
                .zip(&merged.origins)
                .map(|(item, origin)| {
                    let within: Vec<usize> = (0..sides.len())
                        .filter(|side| sides[*side].contains(origin))
                        .collect();
                    MergeLine {
                        text: item.text.clone(),
                        origin: origin.to_string(),
                        side: (within.len() < sides.len())
                            .then(|| within.first().copied())
                            .flatten(),
                    }
                })
                .collect();
            files.push(MergeFile {
                file: file.to_string(),
                path: entry.path.clone(),
                lines,
                contests: merged
                    .contested
                    .iter()
                    .map(|contest| ContestSummary {
                        at: contest.at,
                        len: contest.len,
                        kind: match contest.kind {
                            Contest::Insertion => "insertion",
                            Contest::Deletion => "deletion",
                            Contest::Terminator => "terminator",
                        },
                        revisions: contest.revisions.iter().map(ToString::to_string).collect(),
                    })
                    .collect(),
                hand: hunks(&automatic, &recorded),
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(MergeDetail {
            id: id.to_string(),
            parents: parents.iter().map(ToString::to_string).collect(),
            files,
        })
    }

    /// Every file in the folder as `at` has it, by path.
    pub fn files(&self, at: &str) -> Result<Vec<TreeFile>, Failure> {
        let at: RevisionId = at.parse().map_err(failed)?;
        let tree = self.store.tree(&at).map_err(failed)?;
        let mut files: Vec<TreeFile> = tree
            .entries()
            .map(|(file, entry)| TreeFile {
                file: file.to_string(),
                path: entry.path.clone(),
                kind: kind_name(entry.kind),
                size: entry.size,
            })
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(files)
    }

    /// Every revision that names `file`, newest first.
    ///
    /// A file's identifier outlives its paths, so this follows it through
    /// every move. A merge that changed the file's content without naming it
    /// — taking one side's edits whole — is not a version here: it wrote
    /// nothing of the file's own.
    pub fn file_history(&self, file: &str) -> Result<Vec<Version>, Failure> {
        let file: FileId = file.parse().map_err(failed)?;
        let mut versions: Vec<Version> = Vec::new();
        for id in &self.order {
            let Ok(Some(document)) = self.store.get(id) else {
                continue;
            };
            if let Some(kind) = touches(document, &file) {
                let path = document
                    .added
                    .get(&file)
                    .or_else(|| document.moved.get(&file))
                    .cloned();
                versions.push(Version {
                    revision: id.to_string(),
                    kind,
                    path,
                });
            }
        }
        // A path changes only where a revision adds or moves the file, so
        // every other version takes the path of the one before it. Across
        // concurrent branches that is the nearest one in the listing, which
        // can name the other branch's move.
        let mut carried: Option<String> = None;
        for version in versions.iter_mut().rev() {
            match &version.path {
                Some(path) => carried = Some(path.clone()),
                None => version.path = carried.clone(),
            }
        }
        Ok(versions)
    }

    /// Every revision that names a file under `folder` as `at` has it, newest
    /// first, with how many of those files each one names.
    ///
    /// The folder is the set of files it holds at `at`, followed back through
    /// their own histories. A file that left the folder before `at` is not
    /// counted.
    pub fn folder_history(&self, at: &str, folder: &str) -> Result<Vec<FolderVersion>, Failure> {
        let at: RevisionId = at.parse().map_err(failed)?;
        let tree = self.store.tree(&at).map_err(failed)?;
        let within: BTreeSet<FileId> = tree
            .files()
            .filter(|(_, path)| beneath(path, folder))
            .map(|(file, _)| *file)
            .collect();
        let mut versions = Vec::new();
        for id in &self.order {
            let Ok(Some(document)) = self.store.get(id) else {
                continue;
            };
            let files = within
                .iter()
                .filter(|file| touches(document, file).is_some())
                .count();
            if files > 0 {
                versions.push(FolderVersion {
                    revision: id.to_string(),
                    files,
                });
            }
        }
        Ok(versions)
    }

    /// One file as `at` has it, or `None` where it is not in that folder.
    pub fn file_at(&self, at: &str, file: &str) -> Result<Option<FileAt>, Failure> {
        let at: RevisionId = at.parse().map_err(failed)?;
        let file: FileId = file.parse().map_err(failed)?;
        let tree = self.store.tree(&at).map_err(failed)?;
        let Some(entry) = tree.entry(&file) else {
            return Ok(None);
        };
        let mut shown = FileAt {
            file: file.to_string(),
            path: entry.path.clone(),
            kind: kind_name(entry.kind),
            text: None,
            size: entry.size,
            target: None,
            held: true,
        };
        match entry.kind {
            Kind::Link => {
                shown.target = entry.target.as_ref().map(|target| {
                    historica::update::materialise(&tree, &entry.path, target)
                        .unwrap_or_else(|| target.to_string())
                });
            }
            _ => match self.store.content_in(&tree, &[at], &file).map_err(failed)? {
                Content::Lines(state) => shown.text = Some(state.text()),
                Content::Whole(payload) => {
                    let bytes = self.store.payload(&payload).map_err(failed)?;
                    shown.held = bytes.is_some();
                    if shown.size.is_none() {
                        shown.size = bytes.map(|bytes| bytes.len() as u64);
                    }
                }
            },
        }
        Ok(Some(shown))
    }

    /// A file of bytes as `at` has it, or `None` where this copy does not
    /// hold them.
    pub fn file_bytes(&self, at: &str, file: &str) -> Result<Option<Vec<u8>>, Failure> {
        let at: RevisionId = at.parse().map_err(failed)?;
        let file: FileId = file.parse().map_err(failed)?;
        let tree = self.store.tree(&at).map_err(failed)?;
        match self.store.content_in(&tree, &[at], &file).map_err(failed)? {
            Content::Whole(payload) => self.store.payload(&payload).map_err(failed),
            Content::Lines(state) => Ok(Some(state.text().into_bytes())),
        }
    }

    /// The lines that changed in `file` between `from` and `to`.
    ///
    /// `from` is `None` for a file's first version, which compares against
    /// nothing. `None` comes back where either side is not lines.
    pub fn file_diff(
        &self,
        from: Option<&str>,
        to: &str,
        file: &str,
    ) -> Result<Option<Vec<Hunk>>, Failure> {
        let side = |at: &str| -> Result<Side, Failure> {
            match self.file_at(at, file)? {
                None => Ok(Side::Absent),
                Some(shown) => Ok(match shown.text {
                    Some(text) if shown.kind == "lines" => Side::Lines(text),
                    _ => Side::Other(None),
                }),
            }
        };
        let old = match from {
            Some(from) => side(from)?,
            None => Side::Absent,
        };
        let new = side(to)?;
        Ok(match (&old, &new) {
            (Side::Other(_), _) | (_, Side::Other(_)) => None,
            _ => Some(hunks(text_or_empty(&old), text_or_empty(&new)).unwrap_or_default()),
        })
    }

    fn document(&self, id: &RevisionId) -> Result<&RevisionDocument, Failure> {
        self.store
            .get(id)
            .map_err(failed)?
            .ok_or_else(|| Failure(format!("no revision {id}")))
    }

    /// `id` and every revision it descends from.
    fn ancestors(&self, id: RevisionId) -> BTreeSet<RevisionId> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![id];
        while let Some(next) = stack.pop() {
            if !seen.insert(next) {
                continue;
            }
            if let Ok(Some(document)) = self.store.get(&next) {
                stack.extend(document.parents.iter().copied());
            }
        }
        seen
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

/// What `document` did to `file`, if it names it at all.
fn touches(document: &RevisionDocument, file: &FileId) -> Option<&'static str> {
    if document.added.contains_key(file) {
        return Some("added");
    }
    if document.dropped.contains(file) {
        return Some("dropped");
    }
    if document.edited.contains_key(file)
        || document.bytes.contains_key(file)
        || document.links.contains_key(file)
    {
        return Some("edited");
    }
    if document.moved.contains_key(file) {
        return Some("moved");
    }
    document.modes.contains_key(file).then_some("mode")
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Lines => "lines",
        Kind::Whole => "bytes",
        _ => "link",
    }
}

/// Whether `path` is inside `folder`, where `""` is the whole folder.
fn beneath(path: &str, folder: &str) -> bool {
    folder.is_empty()
        || path
            .strip_prefix(folder)
            .is_some_and(|rest| rest.starts_with('/'))
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
