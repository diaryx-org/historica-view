//! Which bytes a repository's path is.
//!
//! R2 keeps each file once, under its digest, at `blobs/sha256/<digest>`. A
//! fork shares almost every file with what it was forked from, and so does
//! each push with the last; kept by digest, a fork costs its manifest and
//! what it changes. So a path is not a key in the bucket. It is a line in
//! one repository's own listings, which say what digest that repository
//! serves at it:
//!
//! - **The manifest**, `offer.txt` and its pages, names every file of the
//!   copy `historica fetch` takes, by the digest it is checked against. Its
//!   pages are named by digest too.
//! - **The site listing**, `site.txt`, names the rest of what `publish.sh`
//!   lays out, which the manifest does not: the static view, the git
//!   mirror, the store's `historica.txt`, the exported folder, and
//!   `history.tar`, which the Durable Object opens the store from.
//!
//! A path neither names is not there, whatever the bucket holds. A digest
//! that some other repository serves is no answer to a path this one does
//! not list: resolving reads this repository's listings and nothing else.
//!
//! The listings may only name digests this repository *holds*: ones whose
//! bytes were pushed to it, or that it was forked with. The push API checks
//! that before it takes a listing ([`crate::push`]); this module only reads
//! them.

use std::collections::{BTreeMap, BTreeSet};

use historica_view::published::Listing;
use serde::{Deserialize, Serialize};

use crate::route::Within;

/// The manifest, which is the repository's own object and not a blob: it
/// is what moves when the repository is pushed to.
pub const MANIFEST: &str = "offer.txt";

/// The site listing, the repository's own object beside the manifest, as
/// the hub kept it before a manifest named its listing. A repository not
/// pushed to since still has one, and no metadata on its manifest.
pub const SITE: &str = "site.txt";

/// The manifest's custom metadata naming the site listing it goes with.
pub const SITE_METADATA: &str = "site";

/// Where repository `prefix`'s site listing whose text hashes to `digest`
/// is kept. A push stages its listing here, and it is served only once a
/// manifest names it, so the manifest is the one write that moves the
/// repository.
pub fn staged(prefix: &str, digest: &str) -> String {
    format!("{prefix}sites/{digest}")
}

/// Where the site listing that `manifest`, repository `prefix`'s manifest
/// object, goes with is kept.
pub fn site_of(prefix: &str, manifest: &worker::Object) -> worker::Result<String> {
    Ok(match manifest.custom_metadata()?.get(SITE_METADATA) {
        Some(digest) if digest_ok(digest) => staged(prefix, digest),
        _ => format!("{prefix}{SITE}"),
    })
}

/// The archive of the store that the Durable Object opens, as `push.sh`
/// names it in the site listing.
pub const ARCHIVE: &str = "history.tar";

/// Where a file whose bytes hash to `digest` is kept.
pub fn blob(digest: &str) -> String {
    format!("blobs/sha256/{digest}")
}

/// Whether `digest` is spelled as a SHA-256 digest is here: 64 lowercase
/// hexadecimal characters.
pub fn digest_ok(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// What a path of a repository is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Found {
    /// The repository's manifest.
    Manifest,
    /// The blob with this digest.
    Blob(String),
}

/// One repository's listings, read: every path it serves, and the digest at
/// each.
#[derive(Debug, Default, Clone)]
pub struct Site {
    /// What the manifest names, its pages included.
    manifest: BTreeMap<String, String>,
    /// What the site listing names.
    listed: BTreeMap<String, String>,
    /// The exported folder's name in the site, which `raw/` and the git
    /// mirror are read against.
    export: String,
}

impl Site {
    /// The site that `pages` (as [`Published::pages`] gives them), `listing`
    /// (the manifest's files) and the site listing's `text` describe.
    ///
    /// [`Published::pages`]: historica_view::published::Published::pages
    pub fn new(pages: &[(String, String)], listing: &Listing, text: &str) -> Result<Self, String> {
        let mut manifest: BTreeMap<String, String> = pages.iter().cloned().collect();
        for file in &listing.files {
            manifest.insert(file.path.clone(), file.digest.clone());
        }
        let export = listing
            .header
            .strip_suffix("/history/historica.txt")
            .unwrap_or_default()
            .to_owned();
        Ok(Self {
            manifest,
            listed: listed(text)?,
            export,
        })
    }

    /// What `within` is in this repository, or `None` where it serves
    /// nothing there.
    pub fn resolve(&self, within: &Within) -> Option<Found> {
        let path = match within {
            Within::Site(path) => path.clone(),
            Within::Raw(path) if self.export.is_empty() => path.clone(),
            Within::Raw(path) => format!("{}/{path}", self.export),
            Within::Git(_) if self.export.is_empty() => return None,
            Within::Git(path) => format!("{}.git/{path}", self.export),
            // The hub's own, never a repository's.
            Within::View(_) => return None,
        };
        if path == MANIFEST {
            return Some(Found::Manifest);
        }
        self.manifest
            .get(&path)
            .or_else(|| self.listed.get(&path))
            .map(|digest| Found::Blob(digest.clone()))
    }

    /// The digest of the store's archive, where the site listing names one.
    pub fn archive(&self) -> Option<&str> {
        self.listed.get(ARCHIVE).map(String::as_str)
    }

    /// Every digest this repository serves.
    pub fn named(&self) -> BTreeSet<&str> {
        self.manifest
            .values()
            .chain(self.listed.values())
            .map(String::as_str)
            .collect()
    }
}

/// A site listing read: one `<digest> <path>` a line, as `sha256sum` prints
/// a file's, with a path relative to the site.
///
/// Refused whole for a line that is not one, a path that climbs out, or a
/// path that is the manifest, which is never a blob.
pub fn listed(text: &str) -> Result<BTreeMap<String, String>, String> {
    let mut listed = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let line_number = number + 1;
        let Some((digest, path)) = line.split_once(' ') else {
            return Err(format!("line {line_number} is not `<digest> <path>`"));
        };
        if !digest_ok(digest) {
            return Err(format!(
                "line {line_number}: {digest} is not a SHA-256 digest"
            ));
        }
        if path.is_empty()
            || path.starts_with('/')
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(format!(
                "line {line_number}: {path} is not a path within the site"
            ));
        }
        if path == MANIFEST || path == SITE {
            return Err(format!(
                "line {line_number}: {path} is the repository's own, not a file of the site"
            ));
        }
        listed.insert(path.to_owned(), digest.to_owned());
    }
    Ok(listed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use historica_view::published::Published;

    const REVISION: &str = "ddd05c62ea557aa36e7773ddb31ffab5b6241385767a8dbdd1f11561afdf5ea2";
    const README: &str = "19812277b04a5a988e4dc361617bcb927d4297c47353da93aa992ac007f1f3cf";
    const SECRET: &str = "87428fc522803d31065e7bce3cf03fe475096631e5e07bbd7a0fde60c4cf25c7";
    const VIEW: &str = "b5c913858e7eaaadaacfb3cb4976f9753eddc900f62e5c4653842ff992c4d18a";
    const ARCHIVED: &str = "f4fb787e0eebdaae0668a29f288ad0347c8c9d85df0afbd9628651421c83816b";

    /// A site whose manifest is `entries`, `<kind> <digest> <path>` each.
    fn site(entries: &[(&str, &str, &str)], text: &str) -> Site {
        let mut manifest = format!("historica-offer-1\nhead {REVISION}\n");
        for (kind, digest, path) in entries {
            manifest.push_str(&format!("{kind} {digest} - {path}\n"));
        }
        let published = Published::read(&manifest).expect("a manifest");
        let listing = published.listing().expect("its listing");
        Site::new(&published.pages(), &listing, text).expect("a site")
    }

    fn ours() -> Site {
        site(
            &[
                (
                    "revision",
                    REVISION,
                    "notes/history/revisions/2026-10/first.rev.txt",
                ),
                (
                    "payload",
                    README,
                    "notes/history/operations/2026-10/first/README.md",
                ),
                (
                    "payload",
                    SECRET,
                    "notes/history/operations/2026-10/first/secret.txt",
                ),
            ],
            &format!(
                "{VIEW} index.html\n{ARCHIVED} history.tar\n{README} notes/README.md\n{VIEW} notes.git/info/refs\n"
            ),
        )
    }

    #[test]
    fn a_path_is_what_this_repository_lists_there() {
        let site = ours();
        let at = |within: Within| site.resolve(&within);
        assert_eq!(at(Within::Site("offer.txt".into())), Some(Found::Manifest));
        assert_eq!(
            at(Within::Site(
                "notes/history/operations/2026-10/first/README.md".into()
            )),
            Some(Found::Blob(README.into()))
        );
        assert_eq!(
            at(Within::Site("index.html".into())),
            Some(Found::Blob(VIEW.into()))
        );
        assert_eq!(
            at(Within::Raw("README.md".into())),
            Some(Found::Blob(README.into()))
        );
        assert_eq!(
            at(Within::Git("info/refs".into())),
            Some(Found::Blob(VIEW.into()))
        );
        assert_eq!(site.archive(), Some(ARCHIVED));
        assert_eq!(blob(README), format!("blobs/sha256/{README}"));
    }

    #[test]
    fn a_path_this_repository_does_not_list_is_not_there_whatever_the_bucket_holds() {
        // Another repository serves SECRET; this one never listed it.
        let elsewhere = ours();
        let here = site(
            &[(
                "revision",
                REVISION,
                "notes/history/revisions/2026-10/first.rev.txt",
            )],
            &format!("{VIEW} index.html\n"),
        );
        let path = Within::Site("notes/history/operations/2026-10/first/secret.txt".into());
        assert_eq!(elsewhere.resolve(&path), Some(Found::Blob(SECRET.into())));
        assert_eq!(here.resolve(&path), None);
        assert!(!here.named().contains(SECRET));
        // Nor is a digest a path: asking for a blob by its name finds nothing.
        assert_eq!(here.resolve(&Within::Site(blob(SECRET))), None);
        assert_eq!(here.resolve(&Within::Site(SECRET.into())), None);
    }

    #[test]
    fn a_site_listing_is_refused_whole_for_a_bad_line() {
        for text in [
            "not-a-digest index.html\n".to_owned(),
            format!("{VIEW}\n"),
            format!("{VIEW} ../elsewhere\n"),
            format!("{VIEW} /etc/passwd\n"),
            format!("{VIEW} a//b\n"),
            format!("{VIEW} offer.txt\n"),
            format!("{} index.html\n", VIEW.to_ascii_uppercase()),
        ] {
            assert!(listed(&text).is_err(), "{text:?} was taken");
        }
        assert_eq!(
            listed(&format!("{VIEW} a file with spaces.txt\n")).unwrap()["a file with spaces.txt"],
            VIEW
        );
    }
}
