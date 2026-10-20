//! The front page of a store's folder: its README.
//!
//! A README at the top of the folder is what a reader of a repository looks
//! for first, so the front page is that document, rendered, with the title and
//! description its frontmatter gives. Which revision it is read at is the
//! page's choice, so the front page at any moment is the README as it stood
//! then.
//!
//! Nothing else in the folder is read as anything but a file. A repository
//! that keeps prov's documents, a tracker among them, shows them in Files like
//! everything else.

use historica::core::{FileId, RevisionId};
use historica::store::Content;
use historica::tree::Kind;
use serde::{Deserialize, Serialize};

use crate::document::{Document, render};
use crate::view::{Failure, View};

/// What the front page of a repository shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overview {
    /// The README's title, where its frontmatter gives one.
    pub title: Option<String>,
    pub description: Option<String>,
    pub readme: Option<Readme>,
}

/// The README, rendered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Readme {
    pub file: String,
    pub path: String,
    /// The body as HTML, unsanitised.
    pub html: String,
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
    /// The front page as `at` has it: the README, and the title and
    /// description its frontmatter gives.
    pub fn overview(&self, at: &str) -> Result<Overview, Failure> {
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
        Ok(Overview {
            title,
            description,
            readme,
        })
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
