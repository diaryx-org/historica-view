//! A published copy, read the way `historica fetch` reads it.
//!
//! Decision 0052 has a store published as an `export` with a manifest beside
//! it, conventionally `offer.txt`, because a static host cannot list a
//! directory and the manifest is the listing it cannot give. Decision 0080
//! lets that manifest be paged. This reads either shape and says which files
//! make up the copy, so the page can fetch them from any static host — the
//! same files a fetcher would take, and nothing a host merely happens to
//! serve beside them.
//!
//! Nothing a host serves is believed. A page is hashed against the digest the
//! manifest names it by, and each file against the digest its line names, as
//! `fetch` does; what the manifest itself claims is all that is taken on
//! trust, which is what decision 0048 already accepted of it.

use historica::format::digest;
use historica::store::{Manifest, Offer, Page, PageRef, STORE_DIR};
use serde::Serialize;

use crate::view::Failure;

/// The header file a store opens on, which a listing never names.
pub const HEADER: &str = "historica.txt";

/// A manifest, and the pages it still needs.
#[derive(Debug)]
pub struct Published {
    shape: Shape,
}

#[derive(Debug)]
enum Shape {
    Whole(Offer),
    Paged {
        heads: Vec<historica::core::RevisionId>,
        refs: Vec<PageRef>,
        pages: Vec<Option<Page>>,
    },
}

/// One file of the copy, where the page fetches it from and where it goes in
/// the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listed {
    /// Relative to the manifest's directory.
    pub path: String,
    /// Relative to the store's own directory, `history/`.
    pub store: String,
    /// What its bytes must hash to.
    pub digest: String,
}

/// Every file of a copy.
#[derive(Debug, Serialize)]
pub struct Listing {
    /// The store's header, relative to the manifest's directory.
    pub header: String,
    pub files: Vec<Listed>,
}

impl Published {
    /// Read a manifest in either shape.
    pub fn read(text: &str) -> Result<Self, Failure> {
        let shape = match Manifest::parse(text).map_err(|error| Failure(error.to_string()))? {
            Manifest::Whole(offer) => Shape::Whole(offer),
            Manifest::Paged(tip) => Shape::Paged {
                heads: tip.heads().to_vec(),
                refs: tip.pages().to_vec(),
                pages: vec![None; tip.pages().len()],
            },
            _ => {
                return Err(Failure(
                    "a manifest of a shape this view cannot read".into(),
                ));
            }
        };
        Ok(Self { shape })
    }

    /// The pages still to fetch, relative to the manifest's directory.
    pub fn wanted(&self) -> Vec<String> {
        match &self.shape {
            Shape::Whole(_) => Vec::new(),
            Shape::Paged { refs, pages, .. } => refs
                .iter()
                .zip(pages)
                .filter(|(_, page)| page.is_none())
                .map(|(page, _)| page.path.clone())
                .collect(),
        }
    }

    /// Every page the manifest names, as `(path, digest)`: where it is,
    /// relative to the manifest's directory, and what its bytes must hash
    /// to. None for a whole listing.
    ///
    /// A host that keeps files by digest fetches a page by the second, and
    /// [`Published::page`] still checks the bytes against it.
    pub fn pages(&self) -> Vec<(String, String)> {
        match &self.shape {
            Shape::Whole(_) => Vec::new(),
            Shape::Paged { refs, .. } => refs
                .iter()
                .map(|page| (page.path.clone(), page.digest.to_string()))
                .collect(),
        }
    }

    /// Take one page the manifest names, refusing bytes that are not the
    /// page it named.
    pub fn page(&mut self, path: &str, bytes: &[u8]) -> Result<(), Failure> {
        let Shape::Paged { refs, pages, .. } = &mut self.shape else {
            return Err(Failure(format!("{path}: a whole listing has no pages")));
        };
        let Some(at) = refs.iter().position(|page| page.path == path) else {
            return Err(Failure(format!("{path}: not a page this manifest names")));
        };
        if digest(bytes) != refs[at].digest {
            return Err(Failure(format!(
                "{path}: not the page the manifest names; it has changed since"
            )));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| Failure(format!("{path}: not text")))?;
        pages[at] = Some(Page::parse(text).map_err(|error| Failure(format!("{path}: {error}")))?);
        Ok(())
    }

    /// Every file of the copy, once every page has arrived.
    pub fn listing(&self) -> Result<Listing, Failure> {
        let composed;
        let offer = match &self.shape {
            Shape::Whole(offer) => offer,
            Shape::Paged { heads, pages, refs } => {
                let held: Vec<Page> = pages.iter().flatten().cloned().collect();
                if held.len() != refs.len() {
                    return Err(Failure("a page the manifest names has not arrived".into()));
                }
                composed = Offer::composed(heads.clone(), &held);
                &composed
            }
        };
        let marker = format!("{STORE_DIR}/");
        let prefix = offer
            .entries()
            .iter()
            .find_map(|entry| store_prefix(&entry.path, &marker))
            .unwrap_or_default();
        let within = format!("{prefix}{marker}");
        let files = offer
            .entries()
            .iter()
            .filter_map(|entry| {
                Some(Listed {
                    store: entry.path.strip_prefix(&within)?.to_owned(),
                    path: entry.path.clone(),
                    digest: entry.digest.to_string(),
                })
            })
            .collect();
        Ok(Listing {
            header: format!("{within}{HEADER}"),
            files,
        })
    }
}

/// What comes before the store's own directory in a listed path: the name of
/// the exported folder the manifest sits beside, as `store/`, or nothing.
///
/// Read off a revision document's path, whose shape is fixed — the store's
/// directory, then `revisions/` — where any other kind's path could hold a
/// directory called `history` of its own further down.
fn store_prefix<'a>(path: &'a str, marker: &str) -> Option<&'a str> {
    let revisions = format!("{marker}revisions/");
    let at = if path.starts_with(&revisions) {
        0
    } else {
        path.find(&format!("/{revisions}"))? + 1
    };
    Some(&path[..at])
}

/// Whether `bytes` are what a listed file's digest says.
pub fn verified(bytes: &[u8], expected: &str) -> bool {
    digest(bytes).to_string() == expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefix_is_the_exported_folder() {
        assert_eq!(
            store_prefix("store/history/revisions/2026-08/a.rev.txt", "history/"),
            Some("store/")
        );
        assert_eq!(
            store_prefix("history/revisions/a.rev.txt", "history/"),
            Some("")
        );
        assert_eq!(
            store_prefix("store/history/operations/x/history/y", "history/"),
            None
        );
    }
}
