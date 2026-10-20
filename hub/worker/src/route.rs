//! What a path asks for.
//!
//! A repository is `/<owner>/<slug>`: the owner is an account's handle, and
//! the slug is the repository's name among that account's. Two accounts can
//! each have a `notes`, and a fork is the same slug under the contributor's
//! handle. Everything about one repository is under its two segments:
//!
//! ```text
//! /<owner>/<slug>               its front page
//! /<owner>/<slug>/<path>        a file of the published site: offer.txt,
//!                               its pages, the export
//! /<owner>/<slug>/browse/…      the static view, for History and Files:
//!                               the hub's own copy of the page, reading
//!                               this repository's files beside it
//! /<owner>/<slug>/raw/<path>    a file of the exported folder, as a
//!                               README links to it
//! /<owner>/<slug>.git/…         the git mirror, for `git clone`
//! /api/repos/<owner>/<slug>/…   pushes, forks and members
//! /api/account                  whose push key a request sends
//! ```
//!
//! Nothing here reads a repository or asks who may: a route only names what
//! was asked for. Which file a path is, is [`crate::site`]'s; who may read
//! it, [`crate::gate`]'s.

/// The path the hub's accounts are under, passed to the auth worker.
const AUTH_PREFIX: &str = "/api/auth/";

/// What a request's path asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// `/`: the repositories anyone may read.
    Home,
    /// `/api/page`: a page's data alone, for the browser.
    PageApi,
    /// `/api/auth/…`: better-auth's, as it came.
    Auth,
    /// `/api/account`: who the request is from, which is how a push tool
    /// learns its key's handle.
    AccountApi,
    /// `/api/repos/<owner>/<slug>/…`: a change to a repository.
    Api { repo: Named, rest: Vec<String> },
    /// `/<owner>/<slug>`: a repository's front page.
    Page { repo: Named },
    /// A file of a repository.
    File { repo: Named, within: Within },
    /// Anything else.
    NotHere,
}

/// A repository as a path names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    pub owner: String,
    pub slug: String,
}

impl Named {
    /// `<owner>/<slug>`, as a reader would write it.
    pub fn spelled(&self) -> String {
        format!("{}/{}", self.owner, self.slug)
    }
}

/// Which file of a repository's site a path asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Within {
    /// A path of the site as `publish.sh` lays it out: `offer.txt`, a page
    /// of it, a file of the export, or the static view.
    Site(String),
    /// A path within the exported folder, which is the folder's name in the
    /// site and then this.
    Raw(String),
    /// A path within the git mirror.
    Git(String),
    /// A file of the static view itself, `index.html` or under `assets/`,
    /// which the hub serves from its own copy of the page for every
    /// repository rather than from what was pushed.
    View(String),
}

impl Within {
    /// Whether `path`, under `browse/`, is the view's own: its page or one
    /// of the assets `vite build` writes beside it.
    fn view(path: &str) -> bool {
        path == "index.html" || path.starts_with("assets/")
    }
}

impl Route {
    /// The route for `path`, as the URL has it: percent-escaped, beginning
    /// with `/`.
    pub fn of(path: &str) -> Self {
        if path == crate::PAGE_API {
            return Route::PageApi;
        }
        if path.starts_with(AUTH_PREFIX) {
            return Route::Auth;
        }
        let parts: Vec<String> = path
            .trim_start_matches('/')
            .split('/')
            .map(crate::decode)
            .collect();
        let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
        match parts.as_slice() {
            [""] => Route::Home,
            ["api", "repos", owner, slug, rest @ ..] => match named(owner, slug) {
                Some(repo) => Route::Api {
                    repo,
                    rest: rest.iter().map(|part| (*part).to_owned()).collect(),
                },
                None => Route::NotHere,
            },
            ["api", "account"] => Route::AccountApi,
            ["api", ..] => Route::NotHere,
            [owner, git, rest @ ..] if git.ends_with(".git") => {
                let slug = &git[..git.len() - ".git".len()];
                match (named(owner, slug), inner(rest)) {
                    (Some(repo), Some(path)) => Route::File {
                        repo,
                        within: Within::Git(path),
                    },
                    _ => Route::NotHere,
                }
            }
            [owner, slug] | [owner, slug, ""] => match named(owner, slug) {
                Some(repo) => Route::Page { repo },
                None => Route::NotHere,
            },
            [owner, slug, rest @ ..] => {
                let Some(repo) = named(owner, slug) else {
                    return Route::NotHere;
                };
                let within = match rest {
                    ["browse", inner_path @ ..] => {
                        let path = inner(inner_path);
                        match path.as_deref() {
                            Some("") => Some(Within::View("index.html".to_owned())),
                            Some(path) if Within::view(path) => Some(Within::View(path.to_owned())),
                            _ => path.map(Within::Site),
                        }
                    }
                    ["raw", inner_path @ ..] => inner(inner_path)
                        .filter(|path| !path.is_empty())
                        .map(Within::Raw),
                    _ => inner(rest)
                        .filter(|path| !path.is_empty())
                        .map(Within::Site),
                };
                match within {
                    Some(within) => Route::File { repo, within },
                    None => Route::NotHere,
                }
            }
            _ => Route::NotHere,
        }
    }
}

/// `owner` and `slug`, where both can be what they say.
fn named(owner: &str, slug: &str) -> Option<Named> {
    (handle_ok(owner) && slug_ok(slug)).then(|| Named {
        owner: owner.to_owned(),
        slug: slug.to_owned(),
    })
}

/// The rest of a path, joined, where none of it climbs out: `None` for a
/// `..`, a `.`, or an empty segment anywhere but the end.
fn inner(parts: &[&str]) -> Option<String> {
    let last = parts.len().saturating_sub(1);
    for (at, part) in parts.iter().enumerate() {
        if *part == ".." || *part == "." || (part.is_empty() && at != last) {
            return None;
        }
        if part.contains('/') || part.contains('\\') {
            return None;
        }
    }
    Some(parts.join("/"))
}

/// Names the hub keeps for itself, which no account can take as a handle.
/// The auth worker's `usernameValidator` refuses the same list.
pub const RESERVED: &[&str] = &["account", "api", "client"];

/// Whether `handle` can be an account's: 1 to 39 lowercase letters, digits
/// and inner hyphens, as GitHub's are, and none of [`RESERVED`]. The auth
/// worker checks a username against the same rule before it is taken, so
/// every handle in a path is one an account could hold.
pub fn handle_ok(handle: &str) -> bool {
    (1..=39).contains(&handle.len())
        && !handle.starts_with('-')
        && !handle.ends_with('-')
        && handle
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !RESERVED.contains(&handle)
}

/// Whether `slug` can name a repository: a folder name of letters, digits,
/// `-`, `_` and `.`, not hidden, and not ending `.git`, which is the mirror's.
pub fn slug_ok(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 100
        && !slug.starts_with('.')
        && !slug.ends_with(".git")
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(owner: &str, slug: &str) -> Named {
        Named {
            owner: owner.into(),
            slug: slug.into(),
        }
    }

    fn file(owner: &str, slug: &str, within: Within) -> Route {
        Route::File {
            repo: repo(owner, slug),
            within,
        }
    }

    #[test]
    fn a_repository_is_its_owner_and_its_slug() {
        assert_eq!(Route::of("/"), Route::Home);
        for path in ["/ada/notes", "/ada/notes/"] {
            assert_eq!(
                Route::of(path),
                Route::Page {
                    repo: repo("ada", "notes")
                },
                "{path}"
            );
        }
        // A slug alone is no longer a repository.
        assert_eq!(Route::of("/notes"), Route::NotHere);
        // The same slug under another owner is another repository: a fork.
        assert_eq!(
            Route::of("/grace/notes"),
            Route::Page {
                repo: repo("grace", "notes")
            }
        );
    }

    #[test]
    fn every_file_of_a_repository_is_under_its_two_segments() {
        assert_eq!(
            Route::of("/ada/notes/offer.txt"),
            file("ada", "notes", Within::Site("offer.txt".into()))
        );
        assert_eq!(
            Route::of("/ada/notes/notes/history/revisions/2026-10/a%20b.rev.txt"),
            file(
                "ada",
                "notes",
                Within::Site("notes/history/revisions/2026-10/a b.rev.txt".into())
            )
        );
        // The view is the hub's own; what it reads beside itself is the
        // repository's.
        assert_eq!(
            Route::of("/ada/notes/browse/"),
            file("ada", "notes", Within::View("index.html".into()))
        );
        assert_eq!(
            Route::of("/ada/notes/browse/assets/index-a1.js"),
            file("ada", "notes", Within::View("assets/index-a1.js".into()))
        );
        assert_eq!(
            Route::of("/ada/notes/browse/notes/history/historica.txt"),
            file(
                "ada",
                "notes",
                Within::Site("notes/history/historica.txt".into())
            )
        );
        assert_eq!(
            Route::of("/ada/notes/browse/offer.txt"),
            file("ada", "notes", Within::Site("offer.txt".into()))
        );
        assert_eq!(
            Route::of("/ada/notes/raw/docs/a.png"),
            file("ada", "notes", Within::Raw("docs/a.png".into()))
        );
        assert_eq!(
            Route::of("/ada/notes.git/info/refs"),
            file("ada", "notes", Within::Git("info/refs".into()))
        );
    }

    #[test]
    fn the_api_names_a_repository_the_same_way() {
        assert_eq!(Route::of("/api/page"), Route::PageApi);
        assert_eq!(Route::of("/api/auth/get-session"), Route::Auth);
        assert_eq!(Route::of("/api/account"), Route::AccountApi);
        assert_eq!(
            Route::of("/api/repos/ada/notes/fork"),
            Route::Api {
                repo: repo("ada", "notes"),
                rest: vec!["fork".into()]
            }
        );
        assert_eq!(
            Route::of("/api/repos/ada/notes"),
            Route::Api {
                repo: repo("ada", "notes"),
                rest: vec![]
            }
        );
        assert_eq!(Route::of("/api/repos/notes"), Route::NotHere);
        assert_eq!(Route::of("/api/elsewhere"), Route::NotHere);
    }

    #[test]
    fn nothing_climbs_out_or_takes_a_reserved_name() {
        for path in [
            "/ada/notes/../../etc",
            "/ada/notes/raw/../offer.txt",
            "/ada/notes/raw/",
            "/ada/notes/a//b",
            "/ada/notes/a%2Fb",
            "/ada/notes.git/../x",
            "/Ada/notes",
            "/api/notes",
            "/account/notes",
            "/client/notes",
            "/ada/.hidden",
            "/ada/notes.git.git/x",
            "/-ada/notes",
        ] {
            assert_eq!(Route::of(path), Route::NotHere, "{path}");
        }
    }

    #[test]
    fn handles_and_slugs() {
        for handle in ["ada", "a", "ada-l", "x1"] {
            assert!(handle_ok(handle), "{handle}");
        }
        for handle in [
            "",
            "Ada",
            "ada_l",
            "-ada",
            "ada-",
            "a.b",
            "api",
            &"a".repeat(40),
        ] {
            assert!(!handle_ok(handle), "{handle}");
        }
        for slug in ["notes", "historica-view", "a_b", "v1.2"] {
            assert!(slug_ok(slug), "{slug}");
        }
        for slug in ["", ".git", ".hidden", "notes.git", "a b", "a/b"] {
            assert!(!slug_ok(slug), "{slug}");
        }
    }
}
