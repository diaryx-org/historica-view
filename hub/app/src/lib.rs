//! The hub's pages.
//!
//! One set of components, rendered twice. The worker renders them to HTML with
//! `dioxus-ssr` from a [`Page`] it assembled, and embeds that [`Page`] in the
//! document as JSON; the browser reads it back and renders the same
//! components from it, so what the reader first sees and what the client then
//! draws are the same markup. From then on a link is a fetch of the next
//! page's data rather than a page load, under the `client` feature.
//!
//! The components are unit's (diaryx-org/unit), the library Leaf and Diaryx
//! draw with, under one [`unit::UnitRoot`]. Its theme's stylesheet is not in
//! each page: the worker serves [`stylesheet`], unit's and the hub's own
//! together, as one file a browser keeps.
//!
//! This is the shape Dioxus's own fullstack mode has, assembled by hand
//! because its server half does not build for `wasm32-unknown-unknown` yet:
//! `dioxus-server` takes axum with tokio's networking, which a worker does not
//! have. The browser renders afresh rather than hydrating for the same reason
//! — hydration reads the data fullstack's server serialises — and the page
//! data in the document is what stands in for it.

#![forbid(unsafe_code)]

use std::sync::OnceLock;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use unit::prelude::*;

pub use historica_view::workspace::Overview;

mod account;
#[cfg_attr(feature = "client", path = "client.rs")]
#[cfg_attr(not(feature = "client"), path = "server.rs")]
mod client;
#[cfg(feature = "client")]
pub use client::start;

use account::AccountPage;

/// The path the client asks for a page's data at.
pub const PAGE_API: &str = "/api/page";

/// The path the hub's stylesheet is served at, with [`stylesheet_version`]
/// as its query.
pub const STYLESHEET: &str = "/style.css";

/// The element the page renders into.
pub const ROOT: &str = "main";

/// The script element the page's data travels in.
pub const DATA: &str = "hub-page";

/// The hub's own styles, over unit's: the header, the README, the copy
/// commands.
const HUB_CSS: &str = include_str!("hub.css");

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

/// The signed-in account, as its page shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Me {
    pub email: String,
    /// `None` until it chooses one.
    pub handle: Option<String>,
    pub keys: Vec<Key>,
}

/// One of an account's push keys, as better-auth lists it: never the key
/// itself, which is shown once, when it is made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub id: String,
    pub name: Option<String>,
    /// The key's first few characters, to tell it by.
    pub start: Option<String>,
}

/// Everything one page shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "page", rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "one page is held at a time; not `expect`, as the sizes differ by target"
)]
pub enum Page {
    Home {
        repositories: Vec<Listed>,
    },
    Overview {
        repo: Repo,
        overview: Overview,
    },
    /// `/account/`: signing in, the handle, and the push keys.
    Account {
        /// `None` when signed out.
        me: Option<Me>,
        /// The hub's own address, which `historica hub login` is spelled
        /// against.
        origin: String,
        /// A password reset's token, from the link the hub mailed.
        reset: Option<String>,
        /// What a link from the hub's mail came back saying went wrong.
        problem: Option<String>,
    },
    Missing {
        why: String,
    },
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
            Page::Account { .. } => "Account · historica hub".to_owned(),
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
    client::use_history(shown);
    rsx! {
        UnitRoot { stylesheet: false, class: "hub",
            Body { page: shown() }
        }
    }
}

/// The header and the page under it, inside unit's root.
#[component]
fn Body(page: Page) -> Element {
    client::use_timer(use_unit());
    rsx! {
        header { class: "hub-header",
            Link { to: "/", class: "hub-brand", "historica hub" }
            if let Some(repo) = repo_of(&page) {
                nav { class: "hub-repo-nav", "aria-label": "Repository",
                    Link { to: "{repo.path()}", class: "hub-slug", "{repo.owner}/{repo.slug}" }
                    Link { to: "{repo.path()}", "Overview" }
                    a { href: "{repo.path()}/browse/#history/{repo.head.id}", "History" }
                    a { href: "{repo.path()}/browse/#files/{repo.head.id}/d/", "Files" }
                }
            }
            Spacer {}
            Link { to: "/account/", class: "hub-account", "Account" }
        }
        div { class: "hub-page",
            match page {
                Page::Home { repositories } => rsx! { Home { repositories } },
                Page::Overview { repo, overview } => rsx! { RepoOverview { repo, overview } },
                Page::Account { me, origin, reset, problem } => rsx! {
                    AccountPage { me, origin, reset, problem }
                },
                Page::Missing { why } => rsx! {
                    Heading { level: 1, "Not here" }
                    Text { dim: true, "{why}" }
                },
            }
        }
    }
}

fn repo_of(page: &Page) -> Option<&Repo> {
    match page {
        Page::Overview { repo, .. } => Some(repo),
        Page::Home { .. } | Page::Account { .. } | Page::Missing { .. } => None,
    }
}

/// A link within the hub: a plain link to the worker, and a fetch of the next
/// page's data once the browser has taken over.
#[component]
fn Link(to: String, class: Option<String>, children: Element) -> Element {
    let Shown(shown) = use_context::<Shown>();
    let href = to.clone();
    rsx! {
        a {
            href: "{href}",
            class: class.unwrap_or_default(),
            onclick: move |event: MouseEvent| {
                // A modified click opens a tab, as a link always has.
                if client::TAKEN_OVER && event.modifiers().is_empty() {
                    event.prevent_default();
                    client::go(shown, to.clone(), true);
                }
            },
            {children}
        }
    }
}

#[component]
fn Home(repositories: Vec<Listed>) -> Element {
    rsx! {
        Heading { level: 1, "Repositories" }
        Text { dim: true,
            "Each one a Historica repository, pushed with "
            code { "historica hub push" }
            " under its owner's handle: readable at its address, fetchable with "
            code { "historica fetch" }
            " and clonable with "
            code { "git clone" }
            "."
        }
        if repositories.is_empty() {
            Text { dim: true, "Nothing is published here yet." }
        } else {
            Section { class: "hub-listing",
                for listed in repositories {
                    Link {
                        key: "{listed.owner}/{listed.slug}",
                        to: "/{listed.owner}/{listed.slug}",
                        class: "hub-listed",
                        "{listed.owner}/{listed.slug}"
                    }
                }
            }
        }
    }
}

/// A command to copy, with its Copy button.
#[component]
fn Command(label: String, text: String) -> Element {
    let unit = use_unit();
    let shown = text.clone();
    rsx! {
        div { class: "hub-command",
            span { class: "hub-command-label", "{label}" }
            code { title: "{shown}", "{shown}" }
            Button {
                variant: Variant::Ghost,
                label: "Copy {label}",
                onpress: move |_| {
                    let text = text.clone();
                    spawn(async move {
                        if client::copy(&text).await {
                            unit.toast(Toast::new("Copied").lasts(std::time::Duration::from_secs(2)));
                        }
                    });
                },
                "Copy"
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
        Heading { level: 1, "{title}" }
        if let Some(description) = &overview.description {
            p { class: "hub-description", "{description}" }
        }
        div { class: "hub-facts",
            if repo.private {
                Badge { tone: Tone::Warning, "Private" }
            }
            if let Some(parent) = &repo.forked_from {
                span {
                    "Forked from "
                    Link { to: "/{parent}", "{parent}" }
                }
            }
            a { href: "{path}/browse/#history/{repo.head.id}",
                if repo.revisions == 1 { "1 revision" } else { "{repo.revisions} revisions" }
            }
        }
        Section { class: "hub-clone",
            Command { label: "historica", text: "historica fetch {repo.origin}{path}/offer.txt" }
            Command { label: "git", text: "git clone {repo.origin}{path}.git" }
        }
        p { class: "hub-latest",
            a { href: "{path}/browse/#history/{repo.head.id}", code { "{short(&repo.head.id)}" } }
            " {repo.head.title} — {repo.head.author}, {day(&repo.head.when)}"
        }
        if let Some(readme) = &overview.readme {
            Card { class: "hub-readme", title: "{readme.path}",
                article { class: "hub-markdown", dangerous_inner_html: "{readme.html}" }
            }
        } else {
            Text { dim: true, "No README at this revision." }
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

/// Every page's stylesheet: unit's theme and components, then the hub's own.
pub fn stylesheet() -> &'static str {
    static CSS: OnceLock<String> = OnceLock::new();
    CSS.get_or_init(|| format!("{}{HUB_CSS}", Unit::stylesheet(&Theme::default())))
}

/// What [`stylesheet`] is at this build, for its address: a new stylesheet
/// is a new address, so the old one can be kept for good.
pub fn stylesheet_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        // FNV-1a: a fingerprint, not a defence.
        let hash = stylesheet()
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
            });
        format!("{hash:016x}")
    })
}
