//! The workspace a store's folder holds: its README and its tasks.
//!
//! A forge needs an issue tracker, and the repositories this view is for
//! already keep one as files. prov's tasks preset makes a task an ordinary
//! Markdown document with a `status` field, placed in the tree by a `part_of`
//! link that reaches an index titled `Tasks`; closing one is an edit to its
//! status, and the file moves to a `closed/` shelf that is itself part of
//! `Tasks`. So the tracker is prov's to read: the folder as a revision has it
//! is handed to prov in memory, prov finds the workspace's root and walks its
//! spine, and the tracker is what prov's `open-tasks` view starts from,
//! `under('Tasks') && present(status)`. Which revision that is is the page's
//! choice, which makes the tracker's own history the store's.
//!
//! Which statuses count as closed is prov's task vocabulary, `done` and
//! `dropped`, which the org's repositories fix so that one tool can read every
//! repository's tasks. A workspace that declares a vocabulary of its own is
//! not consulted: prov has no way yet to say which of a vocabulary's terms are
//! closed, and its proposal for one (term states) is still a draft. When prov
//! can say it, [`CLOSED`] is what it replaces.
//!
//! Without the `prov` feature, or for a store whose folder is not a prov
//! workspace, there is no tracker, and the rest of the view is unchanged.

use historica::core::{FileId, RevisionId};
use historica::store::Content;
use historica::tree::Kind;
use serde::{Deserialize, Serialize};

use crate::document::{Document, render};
use crate::view::{Failure, View};

/// Statuses that mean a task is finished, from prov's task vocabulary.
const CLOSED: [&str; 2] = ["done", "dropped"];

/// One task, as the list shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub file: String,
    pub path: String,
    pub title: String,
    pub status: String,
    /// Whether the status is one of [`CLOSED`].
    pub closed: bool,
    pub created: Option<String>,
    pub updated: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
}

/// One task with its body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskDetail {
    #[serde(flatten)]
    pub task: Task,
    /// The body as HTML, unsanitised.
    pub html: String,
}

/// What the front page of a repository shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overview {
    /// The root document's title, where it has one.
    pub title: Option<String>,
    pub description: Option<String>,
    pub readme: Option<Readme>,
    pub open_tasks: usize,
    pub closed_tasks: usize,
}

/// The README, rendered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Readme {
    pub file: String,
    pub path: String,
    /// The body as HTML, unsanitised.
    pub html: String,
}

/// What prov's `open-tasks` view starts from, before it leaves out the
/// closed: every document under `Tasks` that states a status. A `Closed
/// tasks` shelf filed under `Tasks` says no status, and is not a task.
#[cfg(feature = "prov")]
const TRACKER: &str = "under('Tasks') && present(status)";

/// Where the folder is put for prov, which wants an absolute root.
#[cfg(feature = "prov")]
const MOUNT: &str = "/workspace";

/// Whether prov might read the file at `path` to find the workspace and walk
/// it: its documents, and the configuration and vocabularies beside them.
/// Anything else stays in the store, unread.
#[cfg(feature = "prov")]
fn prov_reads(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".md",
        ".markdown",
        ".dj",
        ".djot",
        ".yaml",
        ".yml",
        ".json",
        ".toml",
        ".figl",
    ]
    .iter()
    .any(|extension| lower.ends_with(extension))
}

fn is_markdown(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name)
}

impl View {
    /// Every task as `at` has it: open ones first, then closed, each newest
    /// first.
    pub fn tasks(&self, at: &str) -> Result<Vec<Task>, Failure> {
        let at: RevisionId = at.parse().map_err(|error| Failure(format!("{error}")))?;
        let mut tasks = self.tracker(at)?;
        tasks.sort_by(|a, b| {
            a.closed
                .cmp(&b.closed)
                .then_with(|| b.created.cmp(&a.created))
                .then_with(|| a.path.cmp(&b.path))
        });
        Ok(tasks)
    }

    /// One task with its body, or `None` where `file` is not a document at
    /// `at`.
    pub fn task(&self, at: &str, file: &str) -> Result<Option<TaskDetail>, Failure> {
        let Some(shown) = self.file_at(at, file)? else {
            return Ok(None);
        };
        let Some(text) = shown.text else {
            return Ok(None);
        };
        let file: FileId = file.parse().map_err(|error| Failure(format!("{error}")))?;
        let document = Document::read(&text);
        Ok(Some(TaskDetail {
            task: task(&shown.path, &file, |key| document.field(key)),
            html: document.html(),
        }))
    }

    /// The front page as `at` has it: the root document's title, the
    /// README, and how many tasks are open.
    pub fn overview(&self, at: &str) -> Result<Overview, Failure> {
        // A tracker prov cannot read does not take the README down with it;
        // the Tasks view is where it says why.
        let tasks = self.tasks(at).unwrap_or_default();
        let at: RevisionId = at.parse().map_err(|error| Failure(format!("{error}")))?;
        let tree = self
            .store()
            .tree(&at)
            .map_err(|error| Failure(format!("{error}")))?;
        let readme = tree
            .files()
            .filter(|(_, path)| !path.contains('/') && stem(path).eq_ignore_ascii_case("readme"))
            .min_by_key(|(_, path)| (!is_markdown(path), path.len()))
            .map(|(file, path)| (*file, path.to_owned()));
        let readme = match readme {
            None => None,
            Some((file, path)) => self.lines(&tree, at, &file)?.map(|text| {
                let document = Document::read(&text);
                (path, file, document)
            }),
        };
        let (title, description) = readme
            .as_ref()
            .map(|(_, _, document)| (document.field("title"), document.field("description")))
            .unwrap_or_default();
        let readme = readme.map(|(path, file, document)| Readme {
            file: file.to_string(),
            html: if is_markdown(&path) {
                document.html()
            } else {
                render(&format!("```\n{}\n```", document.body))
            },
            path,
        });
        let closed_tasks = tasks.iter().filter(|task| task.closed).count();
        Ok(Overview {
            title,
            description,
            readme,
            open_tasks: tasks.len() - closed_tasks,
            closed_tasks,
        })
    }

    /// Every task `at` has, unsorted: what prov's census of the workspace
    /// reaches under `Tasks` with a status. A folder that is not a prov
    /// workspace has none.
    #[cfg(feature = "prov")]
    fn tracker(&self, at: RevisionId) -> Result<Vec<Task>, Failure> {
        use std::collections::BTreeMap;
        use std::path::{Path, PathBuf};

        use prov::discovery::{Discovery, discover};
        use prov::title::title_text;
        use prov::views::{Expression, ViewSpec, documents, narrow};

        fn fail(error: impl std::fmt::Display) -> Failure {
            Failure(format!("prov: {error}"))
        }
        let tree = self
            .store()
            .tree(&at)
            .map_err(|error| Failure(format!("{error}")))?;
        let mut files = Vec::new();
        let mut ids = BTreeMap::new();
        for (file, path) in tree.files() {
            if !prov_reads(path) {
                continue;
            }
            if let Some(text) = self.lines(&tree, at, file)? {
                files.push((Path::new(MOUNT).join(path), text));
                ids.insert(PathBuf::from(path), *file);
            }
        }
        let fs = prov::InMemoryFs::with_files(files);
        let spec = ViewSpec {
            filter: Some(Expression::parse(TRACKER).expect("the tracker's condition parses")),
            ..ViewSpec::new("tasks", Expression::parse("status").expect("a key"))
        };
        prov::block_on(async {
            let Discovery::Found(found) = discover(&fs, Path::new(MOUNT)).await.map_err(fail)?
            else {
                return Ok(Vec::new());
            };
            let workspace = prov::crossing::open_discovered(&fs, &found)
                .await
                .map_err(fail)?;
            let rows = documents(workspace.graph(), &found.root_doc)
                .await
                .map_err(fail)?;
            let base = found.root_dir.strip_prefix(MOUNT).unwrap_or(Path::new(""));
            Ok(narrow("tasks", &spec, rows)
                .rows
                .into_iter()
                .filter_map(|row| {
                    let path = base.join(&row.path);
                    let file = ids.get(&path)?;
                    Some(task(&path.to_string_lossy(), file, |key| {
                        row.meta.get(key).and_then(title_text)
                    }))
                })
                .collect())
        })
    }

    /// Without prov there is no tracker to read.
    #[cfg(not(feature = "prov"))]
    fn tracker(&self, _: RevisionId) -> Result<Vec<Task>, Failure> {
        Ok(Vec::new())
    }

    /// A file of lines' text, or `None` for bytes and links.
    fn lines(
        &self,
        tree: &historica::tree::Tree,
        at: RevisionId,
        file: &FileId,
    ) -> Result<Option<String>, Failure> {
        if tree
            .entry(file)
            .is_none_or(|entry| entry.kind != Kind::Lines)
        {
            return Ok(None);
        }
        Ok(
            match self
                .store()
                .content_in(tree, &[at], file)
                .map_err(|error| Failure(format!("{error}")))?
            {
                Content::Lines(state) => Some(state.text()),
                Content::Whole(_) => None,
            },
        )
    }
}

/// A task as the list shows it, from the fields `field` reads.
fn task(path: &str, file: &FileId, field: impl Fn(&str) -> Option<String>) -> Task {
    let status = field("status").unwrap_or_default();
    Task {
        file: file.to_string(),
        path: path.to_owned(),
        title: field("title").unwrap_or_else(|| stem(path).replace('-', " ")),
        closed: CLOSED.contains(&status.as_str()),
        status,
        created: field("created"),
        updated: field("updated"),
        author: field("author"),
        description: field("description"),
    }
}
