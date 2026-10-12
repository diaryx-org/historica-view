//! A browser view of a Historica store's history.
//!
//! The page fetches a store's files, hands them to [`Files`], and opens a
//! [`Viewer`] on them. Everything it shows is asked of the `historica` library
//! through [`view::View`]; this module only spells those answers as JSON for
//! the page to read.

#![forbid(unsafe_code)]

pub mod memory;
pub mod view;

use wasm_bindgen::prelude::*;

use crate::memory::Memory;
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

    /// How many files have been added.
    pub fn count(&self) -> usize {
        self.memory.len()
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

    /// What one revision did to each file, as JSON.
    pub fn revision(&self, id: &str) -> Result<String, JsError> {
        let detail = self
            .view
            .revision(id)
            .map_err(|failure| JsError::new(&failure.0))?;
        json(&detail)
    }
}

fn json(value: &impl serde::Serialize) -> Result<String, JsError> {
    serde_json::to_string(value).map_err(|error| JsError::new(&error.to_string()))
}
