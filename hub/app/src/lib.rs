//! The hub's pages.
//!
//! One set of components, rendered twice. The worker renders them to HTML with
//! `dioxus-ssr` from a [`Page`] it assembled, and embeds that [`Page`] in the
//! document as JSON; the browser reads it back and renders the same
//! components from it, so what the reader first sees and what the client then
//! draws are the same markup. From then on a link is a fetch of the next
//! page's data rather than a page load, under the `client` feature.
//!
//! This is the shape Dioxus's own fullstack mode has, assembled by hand
//! because its server half does not build for `wasm32-unknown-unknown` yet:
//! `dioxus-server` takes axum with tokio's networking, which a worker does not
//! have. The browser renders afresh rather than hydrating for the same reason
//! — hydration reads the data fullstack's server serialises — and the page
//! data in the document is what stands in for it.

#![forbid(unsafe_code)]

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

pub use historica_view::workspace::Overview;

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "client")]
pub use client::start;

/// The path the client asks for a page's data at.
pub const PAGE_API: &str = "/api/page";

/// The element the page renders into.
pub const ROOT: &str = "main";

/// The script element the page's data travels in.
pub const DATA: &str = "hub-page";

/// One repository, as every page about it shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Repo {
    /// The handle of the account it belongs to.
    pub owner: String,
    pub slug: String,
    /// Whether only its members may read it.
    pub private: bool,
    /// What it was forked from, as `<owner>/<slug>`.
    pub forked_from: Option<String>,
    /// The hub's own address, which the copy commands are spelled against.
    pub origin: String,
    pub revisions: usize,
    pub head: Head,
}

/// The revision a repository is published at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Head {
    pub id: String,
    pub title: String,
    pub author: String,
    pub when: String,
}

/// A repository the hub holds, as the front page lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listed {
    pub owner: String,
    pub slug: String,
}

impl Repo {
    /// Where it is on the hub: `/<owner>/<slug>`.
    pub fn path(&self) -> String {
        format!("/{}/{}", self.owner, self.slug)
    }
}

/// Everything one page shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "page", rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "one page is held at a time; not `expect`, as the sizes differ by target"
)]
pub enum Page {
    Home { repositories: Vec<Listed> },
    Overview { repo: Repo, overview: Overview },
    Missing { why: String },
}

impl Page {
    /// What the browser's tab says.
    pub fn title(&self) -> String {
        match self {
            Page::Home { .. } => "historica hub".to_owned(),
            Page::Overview { repo, overview } => overview
                .title
                .clone()
                .unwrap_or_else(|| format!("{}/{}", repo.owner, repo.slug)),
            Page::Missing { .. } => "Not here · historica hub".to_owned(),
        }
    }
}

/// The page shown, which the client replaces as the reader moves.
#[derive(Clone, Copy)]
struct Shown(Signal<Page>);

/// The whole document body, for the [`Page`] the virtual DOM was given as its
/// root context: `VirtualDom::new(App).with_root_context(page)`.
#[component]
pub fn App() -> Element {
    let shown = use_signal(consume_context::<Page>);
    use_context_provider(|| Shown(shown));
    #[cfg(feature = "client")]
    client::use_history(shown);
    let page = shown();
    rsx! {
        header {
            Link { to: "/", class: "brand", "historica hub" }
            if let Some(repo) = repo_of(&page) {
                nav { class: "repo-nav",
                    Link { to: "{repo.path()}", class: "slug", "{repo.owner}/{repo.slug}" }
                    Link { to: "{repo.path()}", "Overview" }
                    a { href: "{repo.path()}/browse/#history/{repo.head.id}", "History" }
                    a { href: "{repo.path()}/browse/#files/{repo.head.id}/d/", "Files" }
                }
            }
            a { href: "/account/", class: "account", "Account" }
        }
        div { class: "page",
            match page {
                Page::Home { repositories } => rsx! { Home { repositories } },
                Page::Overview { repo, overview } => rsx! { RepoOverview { repo, overview } },
                Page::Missing { why } => rsx! {
                    h2 { "Not here" }
                    p { class: "quiet", "{why}" }
                },
            }
        }
    }
}

fn repo_of(page: &Page) -> Option<&Repo> {
    match page {
        Page::Overview { repo, .. } => Some(repo),
        Page::Home { .. } | Page::Missing { .. } => None,
    }
}

/// A link within the hub: a plain link to the worker, and a fetch of the next
/// page's data once the browser has taken over.
#[component]
fn Link(to: String, class: Option<String>, children: Element) -> Element {
    #[allow(unused_variables)]
    let Shown(shown) = use_context::<Shown>();
    let href = to.clone();
    rsx! {
        a {
            href: "{href}",
            class: class.unwrap_or_default(),
            onclick: move |event: MouseEvent| {
                #[cfg(feature = "client")]
                {
                    // A modified click opens a tab, as a link always has.
                    if !event.modifiers().is_empty() {
                        return;
                    }
                    event.prevent_default();
                    client::go(shown, to.clone(), true);
                }
                #[cfg(not(feature = "client"))]
                let _ = event;
            },
            {children}
        }
    }
}

#[component]
fn Home(repositories: Vec<Listed>) -> Element {
    rsx! {
        h2 { class: "repository", "Repositories" }
        p { class: "quiet",
            "Each one a Historica repository, published with "
            code { "push.sh" }
            " under its owner's handle: readable at its address, fetchable with "
            code { "historica fetch" }
            " and clonable with "
            code { "git clone" }
            "."
        }
        if repositories.is_empty() {
            p { class: "quiet", "Nothing is published here yet." }
        }
        ul { class: "listing",
            for listed in repositories {
                li { key: "{listed.owner}/{listed.slug}",
                    Link { to: "/{listed.owner}/{listed.slug}", "{listed.owner}/{listed.slug}" }
                }
            }
        }
    }
}

#[component]
fn Command(label: String, text: String) -> Element {
    #[allow(unused_variables)]
    let copied = use_signal(|| false);
    let shown = text.clone();
    rsx! {
        div { class: "command",
            span { class: "quiet", "{label}" }
            code { "{shown}" }
            button {
                r#type: "button",
                class: "copy",
                onclick: move |_| {
                    #[cfg(feature = "client")]
                    client::copy(text.clone(), copied);
                },
                if copied() { "Copied" } else { "Copy" }
            }
        }
    }
}

#[component]
fn RepoOverview(repo: Repo, overview: Overview) -> Element {
    let title = overview
        .title
        .clone()
        .unwrap_or_else(|| format!("{}/{}", repo.owner, repo.slug));
    let path = repo.path();
    rsx! {
        h2 { class: "repository", "{title}" }
        if let Some(description) = &overview.description {
            p { class: "description", "{description}" }
        }
        p { class: "facts",
            if repo.private {
                span { class: "private", "Private · " }
            }
            if let Some(parent) = &repo.forked_from {
                "Forked from "
                Link { to: "/{parent}", "{parent}" }
                " · "
            }
            a { href: "{path}/browse/#history/{repo.head.id}", "{repo.revisions} revisions" }
        }
        section { class: "clone",
            Command { label: "historica", text: "historica fetch {repo.origin}{path}/offer.txt" }
            Command { label: "git", text: "git clone {repo.origin}{path}.git" }
        }
        p { class: "latest-revision",
            a { href: "{path}/browse/#history/{repo.head.id}", code { "{short(&repo.head.id)}" } }
            " {repo.head.title} — {repo.head.author}, {day(&repo.head.when)}"
        }
        if let Some(readme) = &overview.readme {
            div { class: "readme",
                div { class: "readme-name", "{readme.path}" }
                article { class: "markdown", dangerous_inner_html: "{readme.html}" }
            }
        } else {
            p { class: "quiet", "No README at this revision." }
        }
    }
}

fn short(id: &str) -> &str {
    &id[..id.len().min(10)]
}

fn day(stamp: &str) -> &str {
    &stamp[..stamp.len().min(10)]
}

/// The page's data, as the document carries it: JSON that cannot close the
/// script element it sits in.
pub fn embedded(page: &Page) -> String {
    serde_json::to_string(page)
        .unwrap_or_else(|_| "null".to_owned())
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}
