//! The workspace a store's folder holds: its README and its tasks.
//!
//! A forge needs an issue tracker, and the repositories this view is for
//! already keep one as files. prov's tasks preset makes a task an ordinary
//! Markdown document with a `status` field, placed in the tree by a `part_of`
//! link that reaches an index titled `Tasks`; closing one is an edit to its
//! status, and the file moves to a `closed/` shelf that is itself part of
//! `Tasks`. So the tracker is read here exactly as prov's `open-tasks` view
//! reads it — `under('Tasks') && present(status)` — at whichever revision the
//! page is looking at, which makes the tracker's own history the store's.
//!
//! Which statuses count as closed is prov's task vocabulary, `done` and
//! `dropped`, which the org's repositories fix so that one tool can read every
//! repository's tasks. A workspace that declares a vocabulary of its own is
//! not consulted: prov has no way yet to say which of a vocabulary's terms are
//! closed, and its proposal for one (`states:`) is still a draft.

use std::collections::{BTreeMap, BTreeSet};

use historica::core::{FileId, RevisionId};
use historica::store::Content;
use historica::tree::Kind;
use serde::{Deserialize, Serialize};

use crate::document::{Document, Target, link_target, render};
use crate::view::{Failure, View};

/// The title of the index every task is part of.
const TASKS: &str = "Tasks";

/// Statuses that mean a task is finished, from prov's task vocabulary.
const CLOSED: [&str; 2] = ["done", "dropped"];

/// How far a `part_of` chain is followed before it is taken to be a loop
/// that the visited set somehow missed.
const DEPTH: usize = 64;

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

/// Every Markdown document in a folder, read once.
struct Documents {
    by_path: BTreeMap<String, (FileId, Document)>,
}

impl Documents {
    fn find(&self, target: &Target) -> Option<(&str, &Document)> {
        let found = match target {
            Target::Path(path) => self.by_path.get_key_value(path),
            Target::Id(id) => self
                .by_path
                .iter()
                .find(|(_, (_, document))| document.field("id").as_deref() == Some(id)),
            Target::Title(title) => self
                .by_path
                .iter()
                .find(|(_, (_, document))| document.field("title").as_deref() == Some(title)),
        };
        found.map(|(path, (_, document))| (path.as_str(), document))
    }

    /// Whether the document at `path` is part of a document titled `title`,
    /// directly or through others: prov's `under()`.
    fn under(&self, path: &str, title: &str) -> bool {
        let mut seen = BTreeSet::new();
        let mut frontier = vec![path.to_owned()];
        for _ in 0..DEPTH {
            let mut next = Vec::new();
            for at in frontier {
                if !seen.insert(at.clone()) {
                    continue;
                }
                let Some((_, document)) = self.by_path.get(&at) else {
                    continue;
                };
                for link in part_of(document) {
                    let Some(target) = link_target(&link, &at) else {
                        continue;
                    };
                    if let Target::Title(named) = &target
                        && named == title
                    {
                        return true;
                    }
                    let Some((found, parent)) = self.find(&target) else {
                        continue;
                    };
                    if parent.field("title").as_deref() == Some(title) {
                        return true;
                    }
                    next.push(found.to_owned());
                }
            }
            if next.is_empty() {
                return false;
            }
            frontier = next;
        }
        false
    }
}

/// A document's `part_of`, which prov writes as one link or a list of them.
fn part_of(document: &Document) -> Vec<String> {
    match document.meta.as_ref().and_then(|meta| meta.get("part_of")) {
        Some(fig::Value::Str(link)) => vec![link.clone()],
        Some(fig::Value::Seq(links)) => links
            .iter()
            .filter_map(|link| link.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
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
        let documents = self.documents(at)?;
        let mut tasks: Vec<Task> = documents
            .by_path
            .iter()
            .filter(|(path, (_, document))| {
                document.present("status") && documents.under(path, TASKS)
            })
            .map(|(path, (file, document))| task(path, file, document))
            .collect();
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
            task: task(&shown.path, &file, &document),
            html: document.html(),
        }))
    }

    /// The front page as `at` has it: the root document's title, the
    /// README, and how many tasks are open.
    pub fn overview(&self, at: &str) -> Result<Overview, Failure> {
        let tasks = self.tasks(at)?;
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

    fn documents(&self, at: RevisionId) -> Result<Documents, Failure> {
        let tree = self
            .store()
            .tree(&at)
            .map_err(|error| Failure(format!("{error}")))?;
        let mut by_path = BTreeMap::new();
        for (file, path) in tree.files() {
            if !is_markdown(path) {
                continue;
            }
            if let Some(text) = self.lines(&tree, at, file)? {
                by_path.insert(path.to_owned(), (*file, Document::read(&text)));
            }
        }
        Ok(Documents { by_path })
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

fn task(path: &str, file: &FileId, document: &Document) -> Task {
    let status = document.field("status").unwrap_or_default();
    Task {
        file: file.to_string(),
        path: path.to_owned(),
        title: document
            .field("title")
            .unwrap_or_else(|| stem(path).replace('-', " ")),
        closed: CLOSED.contains(&status.as_str()),
        status,
        created: document.field("created"),
        updated: document.field("updated"),
        author: document.field("author"),
        description: document.field("description"),
    }
}
