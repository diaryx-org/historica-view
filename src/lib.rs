//! A browser view of a Historica store's history.
//!
//! The page fetches a store's files, hands them to `Files`, and opens a
//! `Viewer` on them. Where the store is a published copy, `Manifest` says
//! which files those are. Everything it shows is asked of the `historica`
//! library through [`view::View`]; the `bindings` feature only spells those
//! answers as JSON for the page to read, and a Rust host — the hub's worker —
//! asks [`view::View`] directly and leaves it off.

#![forbid(unsafe_code)]

pub mod document;
pub mod memory;
pub mod proposals;
pub mod published;
pub mod view;
pub mod workspace;

#[cfg(feature = "bindings")]
mod bindings;
#[cfg(feature = "bindings")]
pub use bindings::{Files, Manifest, Viewer};
