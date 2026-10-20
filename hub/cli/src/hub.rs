//! The hub's API, as a push uses it.
//!
//! A push is the same three steps `../push.sh` takes:
//!
//! 1. the bytes of each file not sent before, to `…/blobs/<sha256>`;
//! 2. the site listing, `<sha256> <path>` for every file but `offer.txt`;
//! 3. `offer.txt`, the manifest, which moves the repository to what it names.
//!
//! The hub never says whether it holds a blob already. What was sent is
//! remembered beside the site, so a push sends what changed since the last;
//! where the hub answers 409, lacking something this remembers sending, the
//! push sends everything again, once.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::Write as _;
use std::path::Path;

use serde::Deserialize;
use ureq::Agent;

use crate::site::{Built, MANIFEST};

/// A hub, and the push key its requests carry.
pub struct Hub {
    base: String,
    key: String,
    agent: Agent,
}

/// Why the hub, or the way to it, did not do what was asked.
#[derive(Debug)]
pub struct Refused {
    /// The hub's status, where it answered.
    status: Option<u16>,
    message: String,
}

impl fmt::Display for Refused {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(&self.message)
    }
}

impl Refused {
    fn other(message: impl ToString) -> Self {
        Refused {
            status: None,
            message: message.to_string(),
        }
    }

    /// Whether the hub lacks something a request named.
    fn missing(&self) -> bool {
        self.status == Some(409)
    }
}

impl Hub {
    pub fn new(base: &str, key: &str) -> Self {
        let agent: Agent = Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        Hub {
            base: base.to_owned(),
            key: key.to_owned(),
            agent,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// The handle of the account this key is, or `None` where it has none
    /// yet.
    pub fn handle(&self) -> Result<Option<String>, Refused> {
        #[derive(Deserialize)]
        struct Account {
            username: Option<String>,
        }
        let url = format!("{}/api/account", self.base);
        let response = self
            .agent
            .get(&url)
            .header("authorization", &format!("Bearer {}", self.key))
            .call()
            .map_err(|error| Refused::other(format!("{}: {error}", self.base)))?;
        let (status, text) = read(response)?;
        match status {
            200 => {
                let account: Account = serde_json::from_str(&text).map_err(|error| {
                    Refused::other(format!("{url} did not answer as a hub does: {error}"))
                })?;
                Ok(account.username)
            }
            401 => Err(Refused {
                status: Some(401),
                message: format!(
                    "{} does not take that key: make one at {}/account/, after verifying your email",
                    self.base, self.base
                ),
            }),
            404 => Err(Refused::other(format!(
                "{} is not a hub, or one too old to answer /api/account",
                self.base
            ))),
            status => Err(Refused {
                status: Some(status),
                message: format!("GET /api/account: {status} {}", text.trim()),
            }),
        }
    }

    /// `method` on `path` under the hub, with `body`. Fine for any 2xx.
    fn call(&self, method: &str, path: &str, body: Option<(&str, &[u8])>) -> Result<(), Refused> {
        let url = format!("{}{path}", self.base);
        let bearer = format!("Bearer {}", self.key);
        let sent = match (method, body) {
            ("PUT", Some((kind, bytes))) => self
                .agent
                .put(&url)
                .header("authorization", &bearer)
                .header("content-type", kind)
                .send(bytes),
            ("PUT", None) => self
                .agent
                .put(&url)
                .header("authorization", &bearer)
                .send_empty(),
            _ => unreachable!("a push only puts"),
        };
        let response = sent.map_err(|error| Refused::other(format!("{method} {path}: {error}")))?;
        let (status, text) = read(response)?;
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(Refused {
                status: Some(status),
                message: format!("{method} {path}: {}", text.trim()),
            })
        }
    }
}

fn read(mut response: ureq::http::Response<ureq::Body>) -> Result<(u16, String), Refused> {
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|error| Refused::other(format!("reading the hub's answer: {error}")))?;
    Ok((status, text))
}

/// Push `built` to `<owner>/<slug>`, remembering what was sent in
/// `place`/sent. How many files' bytes went up.
pub fn push(
    hub: &Hub,
    owner: &str,
    slug: &str,
    private: bool,
    built: &Built,
    place: &Path,
) -> Result<usize, Refused> {
    let api = format!("/api/repos/{owner}/{slug}");
    let remembered = place.join("sent");

    // Make the repository, or check it is ours.
    if private {
        hub.call(
            "PUT",
            &api,
            Some(("application/json", br#"{"private":true}"#)),
        )?;
    } else {
        hub.call("PUT", &api, None)?;
    }

    let mut sent: BTreeSet<String> = fs::read_to_string(&remembered)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    let listing = built.listing_text();
    let manifest = fs::read(built.site.join(MANIFEST))
        .map_err(|error| Refused::other(format!("{MANIFEST}: {error}")))?;

    let mut uploaded = 0;
    let mut again = false;
    loop {
        uploaded += blobs(hub, &api, built, &mut sent, &remembered)?;
        let moved = hub
            .call(
                "PUT",
                &format!("{api}/site"),
                Some(("text/plain", listing.as_bytes())),
            )
            .and_then(|()| {
                hub.call(
                    "PUT",
                    &format!("{api}/offer"),
                    Some(("text/plain", &manifest)),
                )
            });
        match moved {
            Ok(()) => break,
            Err(refused) if refused.missing() && !again => {
                eprintln!("historica hub: {refused}; sending every file again");
                sent.clear();
                again = true;
            }
            Err(refused) => return Err(refused),
        }
    }

    // What the hub holds now is what this push listed.
    let listed: BTreeSet<&String> = built.listing.values().collect();
    let text: String = listed.iter().map(|digest| format!("{digest}\n")).collect();
    fs::write(&remembered, text)
        .map_err(|error| Refused::other(format!("{}: {error}", remembered.display())))?;
    Ok(uploaded)
}

/// Send the bytes of every file of `built` whose digest is not in `sent`,
/// each digest once, adding each to `sent` and to `remembered` as it goes,
/// so a push stopped halfway does not send them again.
fn blobs(
    hub: &Hub,
    api: &str,
    built: &Built,
    sent: &mut BTreeSet<String>,
    remembered: &Path,
) -> Result<usize, Refused> {
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(remembered)
        .map_err(|error| Refused::other(format!("{}: {error}", remembered.display())))?;
    let mut uploaded = 0;
    for (path, digest) in &built.listing {
        if sent.contains(digest) {
            continue;
        }
        let bytes = fs::read(built.site.join(path))
            .map_err(|error| Refused::other(format!("{path}: {error}")))?;
        hub.call(
            "PUT",
            &format!("{api}/blobs/{digest}"),
            Some(("application/octet-stream", &bytes)),
        )?;
        writeln!(log, "{digest}")
            .map_err(|error| Refused::other(format!("{}: {error}", remembered.display())))?;
        sent.insert(digest.clone());
        uploaded += 1;
    }
    Ok(uploaded)
}
