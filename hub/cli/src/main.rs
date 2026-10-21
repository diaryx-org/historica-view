//! `historica-hub`: a Historica repository pushed to a hub.
//!
//! historica runs a word it does not know as `historica-<word>` from `PATH`,
//! in the directory `-C` names, so with this on `PATH` the commands are
//! `historica hub login` and `historica hub push`.
//!
//! A push is what `../push.sh` does, without the checkout it needs: the
//! export and its paged manifest, written by `historica` itself; the git
//! mirror, by `historica-git` where it is installed; the store's `history/`
//! as one archive; and then only the files the hub has not been sent, by
//! digest, with the manifest last. The static view is not sent at all: the
//! hub serves its own copy as every repository's `/browse/`.

#![forbid(unsafe_code)]

mod config;
mod hub;
mod site;

use std::env;
use std::io::{self, IsTerminal as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::config::Config;
use crate::hub::{Hub, Refused};

/// The hub a login goes to when none is named.
const DEFAULT_HUB: &str = "https://historica-hub.harrisadam42103.workers.dev";

const HELP: &str = "\
usage: historica hub <command> [<arguments>]

  login [<hub>]            keep a push key for <hub>, and learn its handle.
                           the key is read from HUB_TOKEN, or asked for;
                           make one at <hub>/account/. <hub> is the last
                           one logged in to, or the hub's own address
  logout [<hub>]           forget the key kept for <hub>
  push [[<owner>/]<slug>] (--public | --private) [--hub <hub>]
       [--target <target>]
                           publish this repository at <hub>/<owner>/<slug>:
                           <owner> is your handle and <slug> the folder's
                           name unless given. the first push makes the
                           repository. --public lets anyone read it and
                           --private only you and its members; each push
                           says which, and is asked at a terminal where it
                           does not. <target> is what to publish, `head`
                           unless given. needs
                           `historica` on PATH, and `historica-git` and `git`
                           for the git mirror, which is left out without them

HUB_TOKEN, where it is set, is the push key, in place of the one kept.
";

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    match run(arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("historica hub: {message}\n\n{HELP}");
            ExitCode::from(2)
        }
        Err(Failure::Error(message)) => {
            eprintln!("historica hub: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Why a command stopped.
#[derive(Debug)]
pub enum Failure {
    /// The arguments were not what the command takes.
    Usage(String),
    /// The command was understood and could not be done.
    Error(String),
}

impl Failure {
    pub fn error(message: impl ToString) -> Self {
        Failure::Error(message.to_string())
    }
}

fn run(arguments: Vec<String>) -> Result<(), Failure> {
    let mut arguments = arguments.into_iter();
    match arguments.next().as_deref() {
        None | Some("help" | "-h" | "--help") => {
            print!("{HELP}");
            Ok(())
        }
        Some("login") => login(arguments.collect()),
        Some("logout") => logout(arguments.collect()),
        Some("push") => push(arguments.collect()),
        Some(other) => Err(Failure::Usage(format!("`{other}` is not a command"))),
    }
}

/// A hub's address as it is kept: without a trailing slash.
fn address(hub: &str) -> Result<String, Failure> {
    let hub = hub.trim_end_matches('/');
    if hub.starts_with("https://") || hub.starts_with("http://") {
        Ok(hub.to_owned())
    } else {
        Err(Failure::Usage(format!(
            "`{hub}` is not a hub's address, as https://hub.example.org"
        )))
    }
}

fn login(arguments: Vec<String>) -> Result<(), Failure> {
    let mut config = Config::read()?;
    let hub = match arguments.as_slice() {
        [] => config
            .default
            .clone()
            .unwrap_or_else(|| DEFAULT_HUB.to_owned()),
        [hub] => address(hub)?,
        _ => return Err(Failure::Usage("`login` takes one hub".into())),
    };
    let key = match env::var("HUB_TOKEN").ok().filter(|key| !key.is_empty()) {
        Some(key) => key,
        None => asked(&hub)?,
    };
    let key = key.trim().to_owned();
    if key.is_empty() {
        return Err(Failure::error("no key was given"));
    }
    let handle = Hub::new(&hub, &key).handle().map_err(Failure::error)?;
    config.keep(&hub, &key, handle.as_deref());
    config.write()?;
    match handle {
        Some(handle) => println!("logged in to {hub} as {handle}"),
        None => println!("logged in to {hub}; choose a handle at {hub}/account/ before pushing"),
    }
    Ok(())
}

/// The key, typed at the terminal without being shown, or else read as the
/// first line of standard input, as `… | historica hub login` gives it.
fn asked(hub: &str) -> Result<String, Failure> {
    if io::stdin().is_terminal() {
        return rpassword::prompt_password(format!(
            "a push key for {hub}, made at {hub}/account/: "
        ))
        .map_err(|error| Failure::error(format!("reading the key: {error}")));
    }
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|error| Failure::error(format!("reading the key: {error}")))?;
    Ok(line)
}

fn logout(arguments: Vec<String>) -> Result<(), Failure> {
    let mut config = Config::read()?;
    let hub = match arguments.as_slice() {
        [] => config
            .default
            .clone()
            .ok_or_else(|| Failure::error("not logged in to any hub"))?,
        [hub] => address(hub)?,
        _ => return Err(Failure::Usage("`logout` takes one hub".into())),
    };
    if !config.forget(&hub) {
        return Err(Failure::error(format!("no key is kept for {hub}")));
    }
    config.write()?;
    println!(
        "forgot the key for {hub}; delete it at {hub}/account/ too, if it is no longer wanted"
    );
    Ok(())
}

fn push(arguments: Vec<String>) -> Result<(), Failure> {
    let mut named: Option<String> = None;
    let mut hub: Option<String> = None;
    let mut private: Option<bool> = None;
    let mut target = "head".to_owned();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--hub" => {
                let given = arguments
                    .next()
                    .ok_or_else(|| Failure::Usage("`--hub` wants a hub's address".into()))?;
                hub = Some(address(&given)?);
            }
            flag @ ("--public" | "--private") => {
                let chosen = flag == "--private";
                if private.is_some_and(|private| private != chosen) {
                    return Err(Failure::Usage(
                        "`push` takes `--public` or `--private`, not both".into(),
                    ));
                }
                private = Some(chosen);
            }
            "--target" => {
                target = arguments
                    .next()
                    .ok_or_else(|| Failure::Usage("`--target` wants a target".into()))?;
            }
            other if other.starts_with('-') => {
                return Err(Failure::Usage(format!(
                    "`{other}` is not an argument `push` takes"
                )));
            }
            other if named.is_none() => named = Some(other.to_owned()),
            other => {
                return Err(Failure::Usage(format!(
                    "`push` takes one name, and `{other}` is a second"
                )));
            }
        }
    }

    if private.is_none() && !io::stdin().is_terminal() {
        return Err(Failure::Usage(
            "say who may read the push: `--public` for anyone, `--private` for you and its members"
                .into(),
        ));
    }

    let config = Config::read()?;
    let hub = hub
        .or_else(|| config.default.clone())
        .ok_or_else(|| Failure::error("not logged in: run `historica hub login` first"))?;
    let kept = config.hubs.get(&hub);
    let key = env::var("HUB_TOKEN")
        .ok()
        .filter(|key| !key.is_empty())
        .or_else(|| kept.map(|kept| kept.key.clone()))
        .ok_or_else(|| {
            Failure::error(format!(
                "no key for {hub}: run `historica hub login {hub}` first"
            ))
        })?;
    let client = Hub::new(&hub, &key);

    let repository = repository()?;
    let folder = repository
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_default();
    let (owner, slug) = match named.as_deref().map(|named| named.split_once('/')) {
        Some(Some((owner, slug))) => (owner.to_owned(), slug.to_owned()),
        Some(None) => (handle(&client, kept)?, named.clone().unwrap_or_default()),
        None => (handle(&client, kept)?, folder),
    };
    if owner.is_empty() || slug.is_empty() || slug.contains('/') || slug.starts_with('.') {
        return Err(Failure::Usage(format!(
            "`{owner}/{slug}` is not <owner>/<slug>"
        )));
    }

    let private = match private {
        Some(private) => private,
        None => visibility(&repository, &format!("{hub}/{owner}/{slug}"))?,
    };
    eprintln!(
        "historica hub: pushing {} to {hub}/{owner}/{slug}, {}",
        repository.display(),
        if private { "private" } else { "public" }
    );
    let place = config::state()?
        .join(config::named(&hub))
        .join(&owner)
        .join(&slug);
    let built = site::build(&repository, &place, &slug, &target)?;
    for note in &built.notes {
        eprintln!("historica hub: {note}");
    }
    let sent = hub::push(&client, &owner, &slug, private, &built, &place)
        .map_err(|refused: Refused| Failure::error(refused))?;

    let at = format!("{hub}/{owner}/{slug}");
    println!("pushed {} to {at}: {sent} files sent", repository.display());
    println!("  {at}");
    println!("  historica fetch {at}/offer.txt");
    if built.mirrored {
        println!("  git clone {hub}/{owner}/{slug}.git");
    }
    Ok(())
}

/// Whether the push of `repository` to `at` is private, asked at the
/// terminal: a push says who may read what it publishes, and with no
/// terminal to ask in, `--public` or `--private` says it, as [`push`] checks
/// before anything else.
fn visibility(repository: &Path, at: &str) -> Result<bool, Failure> {
    eprint!(
        "push {} to {at}, readable by anyone (public) or by you and its members (private)? ",
        repository.display()
    );
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| Failure::error(format!("reading the answer: {error}")))?;
    chosen(&answer)
        .ok_or_else(|| Failure::error("nothing was pushed: answer `public` or `private`"))
}

/// An answer to [`visibility`]: whether it says private, or `None` where it
/// says neither.
fn chosen(answer: &str) -> Option<bool> {
    match answer.trim().to_ascii_lowercase().as_str() {
        "public" => Some(false),
        "private" => Some(true),
        _ => None,
    }
}

/// The account's handle: as kept at login, or else asked of the hub, which
/// is how a key given as HUB_TOKEN is known.
fn handle(client: &Hub, kept: Option<&config::Kept>) -> Result<String, Failure> {
    if let Some(handle) = kept.and_then(|kept| kept.handle.clone()) {
        return Ok(handle);
    }
    client.handle().map_err(Failure::error)?.ok_or_else(|| {
        Failure::error(format!(
            "your account has no handle yet: choose one at {}/account/",
            client.base()
        ))
    })
}

/// The repository this is run in.
fn repository() -> Result<PathBuf, Failure> {
    let here = env::current_dir().map_err(Failure::error)?;
    store_above(&here)
}

/// The nearest folder, `here` or above, with a store in `history/`, looking
/// no higher than the top of the git checkout `here` is in. A push publishes
/// what it finds, so a checkout without a store of its own must not reach
/// past itself to one that holds it, as a checkout inside a private folder
/// would.
fn store_above(here: &Path) -> Result<PathBuf, Failure> {
    let mut at: &Path = here;
    loop {
        if at.join("history").join("historica.txt").is_file() {
            return Ok(at.to_path_buf());
        }
        if at.join(".git").exists() {
            return Err(Failure::error(format!(
                "no `history` directory in {} or above it in its git checkout, {}; \
                 run `historica hub push` where the store is, or `historica init` to make one",
                here.display(),
                at.display()
            )));
        }
        at = at.parent().ok_or_else(|| {
            Failure::error(format!(
                "no `history` directory here or above {}; `historica init` makes one",
                here.display()
            ))
        })?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A fresh folder under the system's temporary directory, named for the
    /// test and this process.
    fn scratch(name: &str) -> PathBuf {
        let path =
            env::temp_dir().join(format!("agent-historica-hub-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn store(at: &Path) {
        fs::create_dir_all(at.join("history")).unwrap();
        fs::write(at.join("history/historica.txt"), "historica-1\n").unwrap();
    }

    #[test]
    fn only_a_whole_word_chooses_who_may_read() {
        assert_eq!(chosen("public\n"), Some(false));
        assert_eq!(chosen(" Private \n"), Some(true));
        assert_eq!(chosen("\n"), None);
        assert_eq!(chosen("y\n"), None);
        assert_eq!(chosen("pub\n"), None);
    }

    #[test]
    fn a_store_is_found_here_or_above_within_the_checkout() {
        let root = scratch("above");
        store(&root);
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("notes/deeper")).unwrap();
        assert_eq!(store_above(&root).unwrap(), root);
        assert_eq!(store_above(&root.join("notes/deeper")).unwrap(), root);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_checkout_without_a_store_does_not_reach_the_one_around_it() {
        // ~/org holds a store; ~/org/historica-view is a checkout of its own.
        let root = scratch("nested");
        store(&root);
        fs::create_dir_all(root.join("view/.git")).unwrap();
        fs::create_dir_all(root.join("view/src")).unwrap();
        assert!(store_above(&root.join("view")).is_err());
        assert!(store_above(&root.join("view/src")).is_err());
        // A worktree's `.git` is a file, and it is a checkout all the same.
        fs::create_dir_all(root.join("worktree")).unwrap();
        fs::write(root.join("worktree/.git"), "gitdir: elsewhere\n").unwrap();
        assert!(store_above(&root.join("worktree")).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
