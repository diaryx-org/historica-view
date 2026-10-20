//! What is kept between runs: each hub's push key, and the copy each push
//! was made from.
//!
//! The keys are in `hubs.json` under the user's configuration directory —
//! `$HISTORICA_HUB_CONFIG` where it is set, else
//! `$XDG_CONFIG_HOME/historica-hub`, else `~/.config/historica-hub` — readable
//! by the user alone. What a push made is under the cache directory, as
//! historica keeps its own: `$HISTORICA_HUB_STATE`, else
//! `$XDG_CACHE_HOME/historica-hub`, else the platform's.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::Failure;

/// The keys kept, one per hub.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// The hub last logged in to, which a push goes to unless told.
    pub default: Option<String>,
    pub hubs: BTreeMap<String, Kept>,
}

/// One hub's key, and the handle its account had at login.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kept {
    pub key: String,
    pub handle: Option<String>,
}

impl Config {
    /// What is kept, or nothing yet.
    pub fn read() -> Result<Self, Failure> {
        let path = file()?;
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| Failure::error(format!("{}: {error}", path.display()))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(Failure::error(format!("{}: {error}", path.display()))),
        }
    }

    /// Keep `key` for `hub`, which becomes the hub a push goes to.
    pub fn keep(&mut self, hub: &str, key: &str, handle: Option<&str>) {
        self.hubs.insert(
            hub.to_owned(),
            Kept {
                key: key.to_owned(),
                handle: handle.map(str::to_owned),
            },
        );
        self.default = Some(hub.to_owned());
    }

    /// Forget `hub`'s key; whether there was one.
    pub fn forget(&mut self, hub: &str) -> bool {
        let had = self.hubs.remove(hub).is_some();
        if self.default.as_deref() == Some(hub) {
            self.default = self.hubs.keys().next().cloned();
        }
        had
    }

    /// Write what is kept, readable by this user alone. Written beside and
    /// moved into place, so a run stopped halfway leaves the old file whole.
    pub fn write(&self) -> Result<(), Failure> {
        let path = file()?;
        let directory = path.parent().expect("a file in a directory");
        fs::create_dir_all(directory)
            .map_err(|error| Failure::error(format!("{}: {error}", directory.display())))?;
        let text = serde_json::to_string_pretty(self).map_err(Failure::error)?;
        let beside = path.with_extension("json.new");
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut out = options
            .open(&beside)
            .map_err(|error| Failure::error(format!("{}: {error}", beside.display())))?;
        out.write_all(text.as_bytes())
            .and_then(|()| out.write_all(b"\n"))
            .and_then(|()| out.sync_all())
            .map_err(|error| Failure::error(format!("{}: {error}", beside.display())))?;
        fs::rename(&beside, &path)
            .map_err(|error| Failure::error(format!("{}: {error}", path.display())))
    }
}

fn given(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn home() -> Result<PathBuf, Failure> {
    given("HOME").ok_or_else(|| Failure::error("HOME is not set"))
}

/// Where the keys are kept.
fn file() -> Result<PathBuf, Failure> {
    let directory = match given("HISTORICA_HUB_CONFIG") {
        Some(directory) => directory,
        None => match given("XDG_CONFIG_HOME").filter(|path| path.is_absolute()) {
            Some(base) => base.join("historica-hub"),
            None => home()?.join(".config/historica-hub"),
        },
    };
    Ok(directory.join("hubs.json"))
}

/// Where what each push made is kept.
pub fn state() -> Result<PathBuf, Failure> {
    if let Some(directory) = given("HISTORICA_HUB_STATE") {
        return Ok(directory);
    }
    if let Some(base) = given("XDG_CACHE_HOME").filter(|path| path.is_absolute()) {
        return Ok(base.join("historica-hub"));
    }
    let home = home()?;
    Ok(if cfg!(target_os = "macos") {
        home.join("Library/Caches/historica-hub")
    } else {
        home.join(".cache/historica-hub")
    })
}

/// A folder name for `hub`, which its address cannot be.
pub fn named(hub: &str) -> String {
    hex(&Sha256::digest(hub.as_bytes()))[..16].to_owned()
}

/// Bytes as lowercase hexadecimal.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_hub_logged_in_to_is_the_default_until_it_is_forgotten() {
        let mut config = Config::default();
        config.keep("https://a.example", "hub_a", Some("ada"));
        config.keep("https://b.example", "hub_b", None);
        assert_eq!(config.default.as_deref(), Some("https://b.example"));
        assert!(config.forget("https://b.example"));
        assert_eq!(config.default.as_deref(), Some("https://a.example"));
        assert!(!config.forget("https://b.example"));
        let text = serde_json::to_string(&config).unwrap();
        let read: Config = serde_json::from_str(&text).unwrap();
        assert_eq!(
            read.hubs["https://a.example"].handle.as_deref(),
            Some("ada")
        );
    }

    #[test]
    fn a_hub_is_named_by_a_digest_of_its_address() {
        assert_eq!(named("https://a.example").len(), 16);
        assert_ne!(named("https://a.example"), named("https://b.example"));
    }
}
