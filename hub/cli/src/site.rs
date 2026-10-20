//! The site a push sends, laid out as `../../publish.sh` lays one out, less
//! the static view, and with the store's `history/` as one archive:
//!
//! ```text
//! <slug>/               `historica export`: the folder and its history
//! offer.txt             `historica offer --into`: the paged manifest
//! offer-pages/          the pages it names
//! <slug>.git/           the git mirror, where historica-git is installed
//! history.tar           <slug>/history, archived
//! ```
//!
//! It is kept between pushes, so each step brings the last push's copy up to
//! date rather than writing it afresh, as publish.sh does.

use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest as _, Sha256};

use crate::Failure;
use crate::config::hex;

/// The manifest, which is sent last and is not in the listing.
pub const MANIFEST: &str = "offer.txt";
/// The archive the hub's Durable Object opens the store from.
pub const ARCHIVE: &str = "history.tar";

/// A site, built.
#[derive(Debug)]
pub struct Built {
    /// Its folder.
    pub site: PathBuf,
    /// Every file but the manifest, by path within the site, with its digest.
    pub listing: BTreeMap<String, String>,
    /// Whether it has a git mirror.
    pub mirrored: bool,
    /// What a person should know about how it was built.
    pub notes: Vec<String>,
}

impl Built {
    /// The site listing as the hub takes it: `<digest> <path>` a line.
    pub fn listing_text(&self) -> String {
        self.listing
            .iter()
            .map(|(path, digest)| format!("{digest} {path}\n"))
            .collect()
    }
}

/// Build the site for `repository` at `place`/site, publishing `target`
/// under `slug`.
pub fn build(repository: &Path, place: &Path, slug: &str, target: &str) -> Result<Built, Failure> {
    let site = place.join("site");
    fs::create_dir_all(&site)
        .map_err(|error| Failure::error(format!("{}: {error}", site.display())))?;
    let historica = env::var_os("HISTORICA").unwrap_or_else(|| "historica".into());
    let mut notes = Vec::new();

    // 1. The copy and its manifest, by historica itself, so that `target`
    // means what it means to `historica export`.
    run(
        Command::new(&historica)
            .arg("-C")
            .arg(repository)
            .arg("export")
            .arg(site.join(slug))
            .arg(target),
        "historica export",
    )?;
    run(
        Command::new(&historica)
            .arg("-C")
            .arg(&site)
            .args(["offer", slug, "--into", MANIFEST]),
        "historica offer",
    )?;

    // 2. The git mirror.
    let bare = site.join(format!("{slug}.git"));
    let mirrored = match mirror(&site.join(slug), &place.join("git"), &bare)? {
        Mirror::Made => true,
        Mirror::Missing(tool) => {
            // A mirror left from an earlier push would be served stale.
            if bare.exists() {
                fs::remove_dir_all(&bare)
                    .map_err(|error| Failure::error(format!("{}: {error}", bare.display())))?;
            }
            notes.push(format!(
                "no git mirror: `{tool}` is not on PATH, so `git clone` will not reach this push"
            ));
            false
        }
    };

    // 3. The store's history, as one archive.
    let archive = archive(&site.join(slug).join("history"), slug)?;
    fs::write(site.join(ARCHIVE), archive)
        .map_err(|error| Failure::error(format!("{ARCHIVE}: {error}")))?;

    let listing = listing(&site)?;
    Ok(Built {
        site,
        listing,
        mirrored,
        notes,
    })
}

/// Run `command`, its output left to the terminal's error stream so that
/// what this prints is the push's own.
fn run(command: &mut Command, what: &str) -> Result<(), Failure> {
    let status = command.stdout(Stdio::null()).status().map_err(|error| {
        Failure::error(match error.kind() {
            io::ErrorKind::NotFound => format!("{what}: not found on PATH"),
            _ => format!("{what}: {error}"),
        })
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(Failure::error(format!("{what} failed")))
    }
}

enum Mirror {
    Made,
    /// The tool that is not installed.
    Missing(&'static str),
}

/// The git mirror of the copy at `copy`, written by `historica-git` into the
/// working repository `work` and pushed into the bare repository `bare`,
/// which is served over plain HTTP.
fn mirror(copy: &Path, work: &Path, bare: &Path) -> Result<Mirror, Failure> {
    let historica_git = env::var_os("HISTORICA_GIT").unwrap_or_else(|| "historica-git".into());
    if !found(&historica_git) {
        return Ok(Mirror::Missing("historica-git"));
    }
    if !found(&OsString::from("git")) {
        return Ok(Mirror::Missing("git"));
    }
    run(
        Command::new(&historica_git)
            .arg("write")
            .arg(copy)
            .arg(work),
        "historica-git write",
    )?;
    if !bare.exists() {
        run(
            Command::new("git")
                .args(["init", "--quiet", "--bare"])
                .arg(bare),
            "git init",
        )?;
    }
    run(
        Command::new("git")
            .arg("-C")
            .arg(work)
            .args(["-c", "push.negotiate=false", "push", "--quiet", "--mirror"])
            .arg(bare),
        "git push",
    )?;
    // The default branch a clone checks out: `main` where there is one.
    let main = Command::new("git")
        .arg("-C")
        .arg(bare)
        .args(["show-ref", "--verify", "--quiet", "refs/heads/main"])
        .status()
        .map_err(Failure::error)?;
    if main.success() {
        run(
            Command::new("git").arg("-C").arg(bare).args([
                "symbolic-ref",
                "HEAD",
                "refs/heads/main",
            ]),
            "git symbolic-ref",
        )?;
    }
    // What dumb HTTP serves in place of a git server.
    run(
        Command::new("git")
            .arg("-C")
            .arg(bare)
            .arg("update-server-info"),
        "git update-server-info",
    )?;
    Ok(Mirror::Made)
}

/// Whether `program` runs at all.
fn found(program: &OsString) -> bool {
    !matches!(
        Command::new(program)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
        Err(error) if error.kind() == io::ErrorKind::NotFound
    )
}

/// `history` as a tar archive whose entries are `<slug>/history/…`, the
/// paths the manifest names. The same files make the same bytes: entries in
/// order, and no time, owner or mode but a file's own executable bit, so a
/// push that changed nothing sends no new archive.
pub fn archive(history: &Path, slug: &str) -> Result<Vec<u8>, Failure> {
    let mut builder = tar::Builder::new(Vec::new());
    let prefix = format!("{slug}/history");
    for (path, full) in files(history)? {
        let bytes = fs::read(&full)
            .map_err(|error| Failure::error(format!("{}: {error}", full.display())))?;
        let mut header = tar::Header::new_ustar();
        header.set_size(bytes.len() as u64);
        header.set_mode(if executable(&full) { 0o755 } else { 0o644 });
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_entry_type(tar::EntryType::Regular);
        builder
            .append_data(&mut header, format!("{prefix}/{path}"), bytes.as_slice())
            .map_err(|error| Failure::error(format!("{ARCHIVE}: {path}: {error}")))?;
    }
    builder
        .into_inner()
        .map_err(|error| Failure::error(format!("{ARCHIVE}: {error}")))
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(_: &Path) -> bool {
    false
}

/// Every file of the site but the manifest, with its digest.
fn listing(site: &Path) -> Result<BTreeMap<String, String>, Failure> {
    let mut listing = BTreeMap::new();
    for (path, full) in files(site)? {
        if path == MANIFEST {
            continue;
        }
        listing.insert(path, digest(&full)?);
    }
    Ok(listing)
}

/// The SHA-256 of the file at `path`, as the hub names a blob.
pub fn digest(path: &Path) -> Result<String, Failure> {
    let mut file = fs::File::open(path)
        .map_err(|error| Failure::error(format!("{}: {error}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| Failure::error(format!("{}: {error}", path.display())))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Every regular file under `root`, by its path within it with `/` between
/// segments, in order. A symbolic link is left out, as `find -type f` leaves
/// it.
fn files(root: &Path) -> Result<BTreeMap<String, PathBuf>, Failure> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| Failure::error(format!("{}: {error}", directory.display())))?;
        for entry in entries {
            let entry = entry.map_err(Failure::error)?;
            let kind = entry.file_type().map_err(Failure::error)?;
            let full = entry.path();
            if kind.is_dir() {
                pending.push(full);
            } else if kind.is_file() {
                let within = full.strip_prefix(root).expect("under the root");
                let spelled = within
                    .components()
                    .map(|part| {
                        part.as_os_str().to_str().map(str::to_owned).ok_or_else(|| {
                            Failure::error(format!(
                                "{} is not a UTF-8 path, which the hub cannot name",
                                full.display()
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join("/");
                found.insert(spelled, full);
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder under the system's temporary directory, named for the
    /// test and this process.
    fn scratch(name: &str) -> PathBuf {
        let path =
            env::temp_dir().join(format!("agent-historica-hub-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn the_same_history_archives_to_the_same_bytes() {
        let root = scratch("archive");
        fs::create_dir_all(root.join("revisions/2026-10")).unwrap();
        fs::write(root.join("historica.txt"), "historica-1\n").unwrap();
        fs::write(root.join("revisions/2026-10/a.rev.txt"), "a\n").unwrap();
        let first = archive(&root, "notes").unwrap();
        let second = archive(&root, "notes").unwrap();
        assert_eq!(first, second);

        let mut names = Vec::new();
        let mut read = tar::Archive::new(first.as_slice());
        for entry in read.entries().unwrap() {
            let entry = entry.unwrap();
            names.push(entry.path().unwrap().to_string_lossy().into_owned());
        }
        assert_eq!(
            names,
            [
                "notes/history/historica.txt",
                "notes/history/revisions/2026-10/a.rev.txt"
            ]
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_listing_is_every_file_but_the_manifest_by_digest() {
        let root = scratch("listing");
        fs::create_dir_all(root.join("offer-pages")).unwrap();
        fs::write(root.join(MANIFEST), "historica-offer-1\n").unwrap();
        fs::write(root.join("offer-pages/a page.txt"), "").unwrap();
        let listing = listing(&root).unwrap();
        assert_eq!(listing.len(), 1);
        // The SHA-256 of nothing.
        assert_eq!(
            listing["offer-pages/a page.txt"],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
