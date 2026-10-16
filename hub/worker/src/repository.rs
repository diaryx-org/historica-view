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
//! What it loads is what `push.sh` uploaded: the paged manifest beside the
//! copy, its pages, and the copy's `history/` as one archive. Every file the
//! manifest lists is taken from the archive and hashed against the digest the
//! manifest names it by, so an archive that disagrees with the manifest is
//! refused rather than half-believed — the same check `historica fetch`
//! makes, and the page makes, of a copy anywhere.
//!
//! [`Filesystem`]: historica::fs::Filesystem

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::Path;
use std::rc::Rc;

use historica_hub_app::{Head, Page, Repo, TaskDetail};
use historica_view::memory::Memory;
use historica_view::published::{Published, verified};
use historica_view::view::{ROOT, View};
use worker::{DurableObject, Env, Request, Response, Result, State, durable_object};

use crate::{ARCHIVE, BUCKET, prefix};

/// A repository as it was loaded, and the manifest it was loaded from.
struct Loaded {
    /// The manifest's etag: when R2's differs, the repository has been pushed
    /// to since.
    etag: String,
    view: View,
    revisions: usize,
    head: Head,
}

#[durable_object]
pub struct Repository {
    env: Env,
    loaded: RefCell<Option<Rc<Loaded>>>,
}

impl DurableObject for Repository {
    fn new(_state: State, env: Env) -> Self {
        Self {
            env,
            loaded: RefCell::new(None),
        }
    }

    /// `GET /page?slug=…&path=…&origin=…`: the [`Page`] at a repository's
    /// path, as JSON. Always `200`; a page that is not there is
    /// [`Page::Missing`], and the worker answers it with a `404`.
    async fn fetch(&self, request: Request) -> Result<Response> {
        let url = request.url()?;
        let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let (Some(slug), Some(path), Some(origin)) =
            (query.get("slug"), query.get("path"), query.get("origin"))
        else {
            return Response::error("slug, path and origin are wanted", 400);
        };
        let page = match self.load(slug).await {
            Ok(Some(loaded)) => page(&loaded, slug, path, origin),
            Ok(None) => missing(format!("Nothing is published as {slug}.")),
            Err(failure) => missing(format!("{slug} could not be read: {failure}")),
        };
        Response::from_json(&page)
    }
}

impl Repository {
    async fn load(&self, slug: &str) -> Result<Option<Rc<Loaded>>> {
        let bucket = self.env.bucket(BUCKET)?;
        let prefix = prefix(slug);
        let Some(manifest) = bucket.head(format!("{prefix}offer.txt")).await? else {
            return Ok(None);
        };
        let etag = manifest.http_etag();
        if let Some(loaded) = self.loaded.borrow().as_ref()
            && loaded.etag == etag
        {
            return Ok(Some(loaded.clone()));
        }

        let read = |key: String| {
            let bucket = &bucket;
            async move {
                let object = bucket.get(&key).execute().await?;
                let Some(body) = object.as_ref().and_then(|object| object.body()) else {
                    return Err(worker::Error::RustError(format!(
                        "{key} is not in the bucket"
                    )));
                };
                body.bytes().await
            }
        };
        let text = String::from_utf8(read(format!("{prefix}offer.txt")).await?)
            .map_err(|_| worker::Error::RustError("the manifest is not text".into()))?;
        let mut published = Published::read(&text).map_err(failed)?;
        for page in published.wanted() {
            let bytes = read(format!("{prefix}{page}")).await?;
            published.page(&page, &bytes).map_err(failed)?;
        }
        let listing = published.listing().map_err(failed)?;

        let mut archived: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let archive = read(format!("{prefix}{ARCHIVE}")).await?;
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
            etag,
            revisions: graph.revisions.len(),
            view,
            head,
        });
        *self.loaded.borrow_mut() = Some(loaded.clone());
        Ok(Some(loaded))
    }
}

/// The page at `path`, which begins with `/<slug>`.
fn page(loaded: &Loaded, slug: &str, path: &str, origin: &str) -> Page {
    let repo = Repo {
        slug: slug.to_owned(),
        origin: origin.to_owned(),
        revisions: loaded.revisions,
        head: loaded.head.clone(),
    };
    let at = loaded.head.id.as_str();
    let rest: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .skip(1)
        .filter(|part| !part.is_empty())
        .collect();
    let answered = (|| -> std::result::Result<Page, String> {
        let tasks = loaded.view.tasks(at).map_err(|failure| failure.0)?;
        let links = Links::new(slug, &tasks);
        Ok(match rest.as_slice() {
            [] => {
                let mut overview = loaded.view.overview(at).map_err(|failure| failure.0)?;
                if let Some(readme) = overview.readme.as_mut() {
                    readme.html = links.sanitise(&readme.html, &readme.path);
                }
                Page::Overview { repo, overview }
            }
            ["tasks"] => Page::Tasks {
                repo,
                tasks,
                closed: false,
            },
            ["tasks", "closed"] => Page::Tasks {
                repo,
                tasks,
                closed: true,
            },
            ["tasks", file] => match loaded.view.task(at, file) {
                Ok(Some(task)) => Page::Task {
                    task: TaskDetail {
                        html: links.sanitise(&task.html, &task.task.path),
                        task: task.task,
                    },
                    repo,
                },
                _ => missing(format!("{slug} has no task {file}.")),
            },
            _ => missing(format!("{slug} has no page {path}.")),
        })
    })();
    answered.unwrap_or_else(missing)
}

fn missing(why: String) -> Page {
    Page::Missing { why }
}

fn failed(failure: historica_view::view::Failure) -> worker::Error {
    worker::Error::RustError(failure.0)
}

fn io(error: std::io::Error) -> worker::Error {
    worker::Error::RustError(format!("the archive: {error}"))
}

/// Where a document's relative links go on the hub: a task to its page,
/// anything else to the file as the copy holds it.
struct Links {
    slug: String,
    tasks: HashMap<String, String>,
}

impl Links {
    fn new(slug: &str, tasks: &[historica_hub_app::Task]) -> Self {
        Self {
            slug: slug.to_owned(),
            tasks: tasks
                .iter()
                .map(|task| (task.path.clone(), task.file.clone()))
                .collect(),
        }
    }

    /// `html` made safe to show, with its relative links read against the
    /// document at `from`.
    fn sanitise(&self, html: &str, from: &str) -> String {
        let rewrite = Rewrite {
            slug: self.slug.clone(),
            tasks: self.tasks.clone(),
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
}

/// A relative link, rewritten to where the hub shows what it names.
struct Rewrite {
    slug: String,
    tasks: HashMap<String, String>,
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
        let slug = &self.slug;
        Some(Cow::Owned(match self.tasks.get(&resolved) {
            Some(file) => format!("/{slug}/tasks/{file}"),
            None => format!("/{slug}/raw/{resolved}{fragment}"),
        }))
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
