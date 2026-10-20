//! One repository, open in memory.
//!
//! historica reads a store through a synchronous [`Filesystem`], and R2 is
//! asynchronous, so a store cannot be opened over R2 the way it is opened over
//! a disk: its files have to be gathered first, as the browser gathers them.
//! A worker that did that on every request would spend each request reading
//! the whole history. A Durable Object, one per repository, does it once and
//! keeps the store open between requests, and asks R2 only whether the
//! manifest has moved — one `head` — before answering.
//!
//! It answers two things: a page, from the store, and which blob a path of
//! the repository is, from its listings ([`Site`]). The listings are read
//! first and alone, so serving a file never waits on the store.
//!
//! The store is opened from what `push.sh` uploaded: the manifest, its
//! pages, and the store's `history/` as one archive, which the site listing
//! names. Every file the manifest lists is taken from the archive and hashed
//! against the digest the manifest names it by, so an archive that disagrees
//! with the manifest is refused rather than half-believed — the same check
//! `historica fetch` makes, and the page makes, of a copy anywhere.
//!
//! Nothing reaches this but the hub's worker, and the worker reaches it only
//! through a [`Granted`](crate::gate::Granted), which only the read gate
//! makes: a private repository's object answers only for a reader the gate
//! let through. It is named by the repository's id, which is never reused.
//!
//! [`Filesystem`]: historica::fs::Filesystem

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::Path;
use std::rc::Rc;

use historica_hub_app::{Head, Page, Repo};
use historica_view::memory::Memory;
use historica_view::published::{Listing, Published, verified};
use historica_view::view::{ROOT, View};
use worker::{Bucket, DurableObject, Env, Request, Response, Result, State, durable_object};

use crate::BUCKET;
use crate::gate::prefix;
use crate::route::Within;
use crate::site::{MANIFEST, SITE, Site, blob};

/// A repository's listings, as the manifest it read them with left them.
struct Opened {
    /// The manifest's etag: when R2's differs, the repository has been pushed
    /// to since.
    etag: String,
    site: Site,
    listing: Listing,
}

/// A repository's store, open.
struct Loaded {
    /// The etag of the manifest it was opened by.
    etag: String,
    view: View,
    revisions: usize,
    head: Head,
}

#[durable_object]
pub struct Repository {
    env: Env,
    opened: RefCell<Option<Rc<Opened>>>,
    loaded: RefCell<Option<Rc<Loaded>>>,
}

impl DurableObject for Repository {
    fn new(_state: State, env: Env) -> Self {
        Self {
            env,
            opened: RefCell::new(None),
            loaded: RefCell::new(None),
        }
    }

    /// Two questions, each by query:
    ///
    /// - `GET /page?id=…&owner=…&slug=…&private=…&forked=…&origin=…`: the
    ///   repository's front [`Page`], as JSON. Always `200`; a page that is
    ///   not there is [`Page::Missing`], and the worker answers it with a
    ///   `404`.
    /// - `GET /file?id=…&kind=site|raw|git&path=…`: which blob that path is,
    ///   as a [`Found`](crate::site::Found) in JSON, or `404` where the
    ///   repository's listings do not name it.
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let Some(id) = query.get("id").and_then(|id| id.parse::<i64>().ok()) else {
            return Response::error("id is wanted", 400);
        };
        match url.path() {
            "/file" => {
                let path = query.get("path").cloned().unwrap_or_default();
                let within = match query.get("kind").map(String::as_str) {
                    Some("site") => Within::Site(path),
                    Some("raw") => Within::Raw(path),
                    Some("git") => Within::Git(path),
                    _ => return Response::error("kind is site, raw or git", 400),
                };
                let found = match self.open(id).await? {
                    Some(opened) => opened.site.resolve(&within),
                    None => None,
                };
                match found {
                    Some(found) => Response::from_json(&found),
                    None => Response::error("not here", 404),
                }
            }
            "/page" => {
                let (Some(owner), Some(slug), Some(origin)) =
                    (query.get("owner"), query.get("slug"), query.get("origin"))
                else {
                    return Response::error("owner, slug and origin are wanted", 400);
                };
                let shown = Shown {
                    owner: owner.clone(),
                    slug: slug.clone(),
                    origin: origin.clone(),
                    private: query.get("private").is_some_and(|value| value == "1"),
                    forked_from: query
                        .get("forked")
                        .filter(|value| !value.is_empty())
                        .cloned(),
                };
                let spelled = format!("{owner}/{slug}");
                let page = match self.load(id).await {
                    Ok(Some(loaded)) => page(&loaded, &shown),
                    Ok(None) => missing(format!("Nothing is published as {spelled}.")),
                    Err(failure) => missing(format!("{spelled} could not be read: {failure}")),
                };
                Response::from_json(&page)
            }
            _ => Response::error("not here", 404),
        }
    }
}

/// What the page says of a repository beyond its store.
struct Shown {
    owner: String,
    slug: String,
    origin: String,
    private: bool,
    forked_from: Option<String>,
}

impl Repository {
    /// The repository's listings, read again where the manifest has moved.
    /// `None` where nothing has been pushed to it.
    async fn open(&self, id: i64) -> Result<Option<Rc<Opened>>> {
        let bucket = self.env.bucket(BUCKET)?;
        let prefix = prefix(id);
        let Some(manifest) = bucket.head(format!("{prefix}{MANIFEST}")).await? else {
            return Ok(None);
        };
        let etag = manifest.http_etag();
        if let Some(opened) = self.opened.borrow().as_ref()
            && opened.etag == etag
        {
            return Ok(Some(opened.clone()));
        }
        let text = String::from_utf8(read(&bucket, format!("{prefix}{MANIFEST}")).await?)
            .map_err(|_| worker::Error::RustError("the manifest is not text".into()))?;
        let mut published = Published::read(&text).map_err(failed)?;
        let pages = published.pages();
        for (path, digest) in &pages {
            let bytes = read(&bucket, blob(digest)).await?;
            published.page(path, &bytes).map_err(failed)?;
        }
        let listing = published.listing().map_err(failed)?;
        let listed = String::from_utf8(read(&bucket, format!("{prefix}{SITE}")).await?)
            .map_err(|_| worker::Error::RustError("the site listing is not text".into()))?;
        let site = Site::new(&pages, &listing, &listed).map_err(worker::Error::RustError)?;
        let opened = Rc::new(Opened {
            etag,
            site,
            listing,
        });
        *self.opened.borrow_mut() = Some(opened.clone());
        Ok(Some(opened))
    }

    /// The repository's store, opened again where the manifest has moved.
    async fn load(&self, id: i64) -> Result<Option<Rc<Loaded>>> {
        let Some(opened) = self.open(id).await? else {
            return Ok(None);
        };
        if let Some(loaded) = self.loaded.borrow().as_ref()
            && loaded.etag == opened.etag
        {
            return Ok(Some(loaded.clone()));
        }
        let bucket = self.env.bucket(BUCKET)?;
        let listing = &opened.listing;
        let archive = opened.site.archive().ok_or_else(|| {
            worker::Error::RustError("the site listing names no history.tar".into())
        })?;
        let archive = read(&bucket, blob(archive)).await?;
        let mut archived: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut entries = tar::Archive::new(archive.as_slice());
        for entry in entries.entries().map_err(io)? {
            let mut entry = entry.map_err(io)?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let path = entry.path().map_err(io)?.to_string_lossy().into_owned();
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes).map_err(io)?;
            archived.insert(path.trim_start_matches("./").to_owned(), bytes);
        }

        let mut memory = Memory::new();
        let header = archived.remove(&listing.header).ok_or_else(|| {
            worker::Error::RustError(format!("the archive has no {}", listing.header))
        })?;
        memory.insert(Path::new(ROOT).join("historica.txt"), header);
        for file in &listing.files {
            let bytes = archived.remove(&file.path).ok_or_else(|| {
                worker::Error::RustError(format!("the archive has no {}", file.path))
            })?;
            if !verified(&bytes, &file.digest) {
                return Err(worker::Error::RustError(format!(
                    "{} in the archive is not the file the manifest names",
                    file.path
                )));
            }
            memory.insert(Path::new(ROOT).join(&file.store), bytes);
        }
        let view = View::open(memory).map_err(failed)?;

        let graph = view.graph();
        let newest = graph
            .revisions
            .iter()
            .find(|revision| revision.head)
            .or(graph.revisions.first())
            .ok_or_else(|| worker::Error::RustError("the copy has no revisions".into()))?;
        let head = Head {
            id: newest.id.clone(),
            title: newest
                .message
                .trim()
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
            author: newest
                .author
                .split(" <")
                .next()
                .unwrap_or(&newest.author)
                .to_owned(),
            when: newest.when.clone(),
        };
        let loaded = Rc::new(Loaded {
            etag: opened.etag.clone(),
            revisions: graph.revisions.len(),
            view,
            head,
        });
        *self.loaded.borrow_mut() = Some(loaded.clone());
        Ok(Some(loaded))
    }
}

/// The front page of a repository.
fn page(loaded: &Loaded, shown: &Shown) -> Page {
    let repo = Repo {
        owner: shown.owner.clone(),
        slug: shown.slug.clone(),
        origin: shown.origin.clone(),
        private: shown.private,
        forked_from: shown.forked_from.clone(),
        revisions: loaded.revisions,
        head: loaded.head.clone(),
    };
    let at = loaded.head.id.as_str();
    match loaded.view.overview(at) {
        Ok(mut overview) => {
            if let Some(readme) = overview.readme.as_mut() {
                let base = format!("/{}/{}", shown.owner, shown.slug);
                readme.html = sanitise(&base, &readme.html, &readme.path);
            }
            Page::Overview { repo, overview }
        }
        Err(failure) => missing(failure.0),
    }
}

fn missing(why: String) -> Page {
    Page::Missing { why }
}

fn failed(failure: historica_view::view::Failure) -> worker::Error {
    worker::Error::RustError(failure.0)
}

async fn read(bucket: &Bucket, key: String) -> Result<Vec<u8>> {
    let object = bucket.get(&key).execute().await?;
    let Some(body) = object.as_ref().and_then(|object| object.body()) else {
        return Err(worker::Error::RustError(format!(
            "{key} is not in the bucket"
        )));
    };
    body.bytes().await
}

fn io(error: std::io::Error) -> worker::Error {
    worker::Error::RustError(format!("the archive: {error}"))
}

/// `html` made safe to show, with its relative links read against the
/// document at `from` and sent to the file as the copy holds it.
fn sanitise(base: &str, html: &str, from: &str) -> String {
    let rewrite = Rewrite {
        base: base.to_owned(),
        directory: from
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_owned())
            .unwrap_or_default(),
    };
    ammonia::Builder::default()
        .url_relative(ammonia::UrlRelative::Custom(Box::new(rewrite)))
        .add_generic_attributes(["align"])
        .clean(html)
        .to_string()
}

/// A relative link, rewritten to where the hub shows what it names.
struct Rewrite {
    /// `/<owner>/<slug>`.
    base: String,
    directory: String,
}

impl ammonia::UrlRelativeEvaluate<'_> for Rewrite {
    fn evaluate<'url>(&self, url: &'url str) -> Option<Cow<'url, str>> {
        if url.starts_with('#') {
            return Some(Cow::Borrowed(url));
        }
        let (path, fragment) = match url.split_once('#') {
            Some((path, fragment)) => (path, format!("#{fragment}")),
            None => (url, String::new()),
        };
        let path = path.split('?').next().unwrap_or(path);
        let resolved = resolve(path, &self.directory);
        let base = &self.base;
        Some(Cow::Owned(format!("{base}/raw/{resolved}{fragment}")))
    }
}

/// `reference` read against `directory`, as a path from the root.
fn resolve(reference: &str, directory: &str) -> String {
    let mut parts: Vec<String> = match reference.starts_with('/') {
        true => Vec::new(),
        false => directory
            .split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect(),
    };
    for part in reference.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(crate::decode(part)),
        }
    }
    parts.join("/")
}
