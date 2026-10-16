//! A browser view of a Historica store's history.
//!
//! The page fetches a store's files, hands them to [`Files`], and opens a
//! [`Viewer`] on them. Where the store is a published copy, [`Manifest`] says
//! which files those are. Everything it shows is asked of the `historica` library
//! through [`view::View`]; this module only spells those answers as JSON for
//! the page to read.

#![forbid(unsafe_code)]

pub mod document;
pub mod memory;
pub mod published;
pub mod view;
pub mod workspace;

use wasm_bindgen::prelude::*;

use crate::memory::Memory;
use crate::published::{Published, verified};
use crate::view::{ROOT, View};

/// A store's files, gathered by the page before it opens a [`Viewer`].
#[wasm_bindgen]
#[derive(Default)]
pub struct Files {
    memory: Memory,
}

#[wasm_bindgen]
impl Files {
    /// No files yet.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one file, by its path relative to the store's root.
    pub fn add(&mut self, path: &str, bytes: Vec<u8>) {
        let relative = path.trim_start_matches('/');
        self.memory.insert(format!("{ROOT}/{relative}"), bytes);
    }

    /// Add one file a manifest listed, refusing bytes that are not the ones
    /// it named.
    #[wasm_bindgen(js_name = addListed)]
    pub fn add_listed(&mut self, path: &str, bytes: Vec<u8>, digest: &str) -> Result<(), JsError> {
        if !verified(&bytes, digest) {
            return Err(JsError::new(&format!(
                "{path} is not the file the manifest names; the copy has changed since it was listed"
            )));
        }
        self.add(path, bytes);
        Ok(())
    }

    /// How many files have been added.
    pub fn count(&self) -> usize {
        self.memory.len()
    }
}

/// A published copy's manifest, in either shape.
#[wasm_bindgen]
pub struct Manifest {
    published: Published,
}

#[wasm_bindgen]
impl Manifest {
    /// Read the text of a manifest.
    pub fn read(text: &str) -> Result<Manifest, JsError> {
        Ok(Self {
            published: Published::read(text).map_err(fail)?,
        })
    }

    /// The pages still to fetch, relative to the manifest.
    pub fn wanted(&self) -> Vec<String> {
        self.published.wanted()
    }

    /// Take one page, by the path [`Manifest::wanted`] named it by.
    pub fn page(&mut self, path: &str, bytes: Vec<u8>) -> Result<(), JsError> {
        self.published.page(path, &bytes).map_err(fail)
    }

    /// Every file of the copy, as JSON, once every page has arrived.
    pub fn listing(&self) -> Result<String, JsError> {
        json(&self.published.listing().map_err(fail)?)
    }
}

/// An open store.
#[wasm_bindgen]
pub struct Viewer {
    view: View,
}

#[wasm_bindgen]
impl Viewer {
    /// Open the store `files` holds, taking them.
    pub fn open(files: Files) -> Result<Viewer, JsError> {
        let view = View::open(files.memory).map_err(|failure| JsError::new(&failure.0))?;
        Ok(Self { view })
    }

    /// Every revision and bookmark, as JSON.
    pub fn graph(&self) -> Result<String, JsError> {
        json(&self.view.graph())
    }

    /// What one revision did to each file, against `against` or else its
    /// first parent, as JSON.
    pub fn revision(&self, id: &str, against: Option<String>) -> Result<String, JsError> {
        json(&self.view.revision(id, against.as_deref()).map_err(fail)?)
    }

    /// How a merge's parents came together, as JSON.
    pub fn merge(&self, id: &str) -> Result<String, JsError> {
        json(&self.view.merge(id).map_err(fail)?)
    }

    /// Every file in the folder as `at` has it, as JSON.
    pub fn files(&self, at: &str) -> Result<String, JsError> {
        json(&self.view.files(at).map_err(fail)?)
    }

    /// Every revision that names `file`, newest first, as JSON.
    #[wasm_bindgen(js_name = fileHistory)]
    pub fn file_history(&self, file: &str) -> Result<String, JsError> {
        json(&self.view.file_history(file).map_err(fail)?)
    }

    /// Every revision that names a file under `folder` at `at`, as JSON.
    #[wasm_bindgen(js_name = folderHistory)]
    pub fn folder_history(&self, at: &str, folder: &str) -> Result<String, JsError> {
        json(&self.view.folder_history(at, folder).map_err(fail)?)
    }

    /// One file as `at` has it, as JSON, or `null`.
    #[wasm_bindgen(js_name = fileAt)]
    pub fn file_at(&self, at: &str, file: &str) -> Result<String, JsError> {
        json(&self.view.file_at(at, file).map_err(fail)?)
    }

    /// A file's bytes as `at` has them, or `undefined` where they are not held.
    #[wasm_bindgen(js_name = fileBytes)]
    pub fn file_bytes(&self, at: &str, file: &str) -> Result<Option<Vec<u8>>, JsError> {
        self.view.file_bytes(at, file).map_err(fail)
    }

    /// The lines `file` changed between two revisions, as JSON, or `null`.
    #[wasm_bindgen(js_name = fileDiff)]
    pub fn file_diff(&self, from: Option<String>, to: &str, file: &str) -> Result<String, JsError> {
        json(
            &self
                .view
                .file_diff(from.as_deref(), to, file)
                .map_err(fail)?,
        )
    }

    /// Every task as `at` has it, as JSON.
    pub fn tasks(&self, at: &str) -> Result<String, JsError> {
        json(&self.view.tasks(at).map_err(fail)?)
    }

    /// One task with its body, as JSON, or `null`.
    pub fn task(&self, at: &str, file: &str) -> Result<String, JsError> {
        json(&self.view.task(at, file).map_err(fail)?)
    }

    /// The front page as `at` has it, as JSON.
    pub fn overview(&self, at: &str) -> Result<String, JsError> {
        json(&self.view.overview(at).map_err(fail)?)
    }
}

fn fail(failure: view::Failure) -> JsError {
    JsError::new(&failure.0)
}

fn json(value: &impl serde::Serialize) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|error| JsError::new(&error.to_string()))
}
