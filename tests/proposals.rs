//! Proposals: `changes/*` bookmarks the accepted line does not reach, their
//! titles, and the claims over their tips, recorded on disk and read back
//! through the in-memory folder the page uses.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use historica::core::RevisionId;
use historica::record::{Clock as _, Platform, Recording, Restriction, record};
use historica::store::{Bookmark, Name, Store};
use historica::working::Working;
use historica_minisign::trust::{self, Entry};
use historica_minisign::{Role, key, sign};
use historica_view::memory::Memory;
use historica_view::view::{ROOT, View};

const AUTHOR: &str = "Ada Lovelace <ada@example.com>";

fn scratch(test: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("proposals-{test}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a folder");
    root
}

/// The folder recorded with `parents`, carrying `headers`.
fn record_on(
    base: &Path,
    store: &mut Store,
    parents: &[RevisionId],
    message: &str,
    headers: &[(&str, &str)],
) -> RevisionId {
    let working = Working::read(base, store.skipped()).expect("the folder");
    let recording = Recording {
        parents: parents.to_vec(),
        author: AUTHOR.to_owned(),
        when: Platform.now().expect("a clock"),
        message: message.to_owned(),
        moves: Vec::new(),
        at: Vec::new(),
        accepted: BTreeSet::new(),
        only: Restriction::Everything,
        kinds: Default::default(),
        extensions: headers
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>(),
    };
    record(store, &working, &recording, &mut Platform)
        .expect("recording")
        .revision
}

fn write(base: &Path, path: &str, text: &str) {
    let path = base.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn bookmark(store: &mut Store, name: &str, at: RevisionId, private: bool) {
    store
        .set_bookmark(
            name,
            Bookmark {
                target: Name::Revision(at),
                private,
            },
        )
        .expect("a bookmark");
}

fn into_memory(directory: &Path) -> Memory {
    fn walk(memory: &mut Memory, root: &Path, at: &Path) {
        for entry in fs::read_dir(at).expect("a directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(memory, root, &path);
            } else {
                let relative = path.strip_prefix(root).expect("beneath the root");
                memory.insert(Path::new(ROOT).join(relative), fs::read(&path).unwrap());
            }
        }
    }
    let mut memory = Memory::new();
    walk(&mut memory, directory, directory);
    memory
}

/// A claim by `secret` over `revision`, written beside the history. A macro
/// because the secret key's type is minisign's, which nothing here names.
macro_rules! vouch {
    ($store:expr, $revision:expr, $role:expr, $secret:expr, $stem:expr) => {{
        let claim = sign::claim_for(
            $revision,
            $role.parse::<Role>().unwrap(),
            $secret,
            &Platform,
        )
        .expect("a claim");
        sign::write($store.filesystem(), $store.root(), &claim, $stem, $secret).expect("written");
    }};
}

#[test]
fn a_proposal_is_a_shared_change_bookmark_the_accepted_line_does_not_reach() {
    let base = scratch("open").join("repo");
    let mut store = Store::init(base.join("history")).expect("a store");

    write(&base, "README.md", "# Example\n");
    let begin = record_on(&base, &mut store, &[], "Begin", &[]);

    // An open proposal of two revisions, titled by its first one's header.
    write(&base, "docs/tracker.md", "# The tracker\n");
    let explain = record_on(
        &base,
        &mut store,
        &[begin],
        "Explain the tracker\n\nA page for it.",
        &[("diaryx.change-set", "Explain how tasks are kept")],
    );
    write(&base, "docs/tracker.md", "# The tracker\n\nIt is prov's.\n");
    let tip = record_on(&base, &mut store, &[explain], "Say whose it is", &[]);
    bookmark(&mut store, "changes/explain", tip, false);

    // A proposal whose header only repeats its id is titled by its message.
    write(&base, "README.md", "# Example\n\nRead me.\n");
    let quiet = record_on(
        &base,
        &mut store,
        &[begin],
        "Say what to read",
        &[("diaryx.change-set", "quiet")],
    );
    bookmark(&mut store, "changes/quiet", quiet, false);

    // A proposal the accepted line has merged is not open any more.
    write(&base, "NOTES.md", "# Notes\n");
    let taken = record_on(&base, &mut store, &[begin], "Add notes", &[]);
    bookmark(&mut store, "changes/taken", taken, false);
    let merged = record_on(&base, &mut store, &[begin, taken], "Take the notes", &[]);
    bookmark(&mut store, "main", merged, false);

    // A private bookmark is nobody's proposal.
    write(&base, "SECRET.md", "# Mine\n");
    let mine = record_on(&base, &mut store, &[merged], "Mine", &[]);
    bookmark(&mut store, "changes/mine", mine, true);

    // Three claims over the tip: one by a key this copy trusts, one by a key
    // it does not, and one whose signature has been tampered with.
    let trusted = key::mint().expect("a key");
    let stranger = key::mint().expect("a key");
    let forger = key::mint().expect("a key");
    vouch!(store, tip, "reviewer", &trusted, "trusted");
    vouch!(store, tip, "author", &stranger, "stranger");
    vouch!(store, tip, "reviewer", &forger, "forged");
    let signature = base.join("history/claims/forged.claim.txt.minisig");
    let text = fs::read_to_string(&signature).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let line = &mut lines[1];
    let flipped = if line.ends_with('A') { 'B' } else { 'A' };
    line.pop();
    line.push(flipped);
    fs::write(&signature, lines.join("\n") + "\n").unwrap();
    let trusted_key = key::public_of(&trusted).unwrap();
    trust::add(
        store.filesystem(),
        store.root(),
        "grace",
        &Entry {
            key: trusted_key.clone(),
            who: "Grace Hopper <grace@example.com>".to_owned(),
        },
    )
    .expect("trusted");

    let view = View::open(into_memory(&base.join("history"))).expect("the store opens");
    let proposals = view.proposals().expect("the proposals");
    let mut names: Vec<&str> = proposals.iter().map(|p| p.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["changes/explain", "changes/quiet"]);
    let named = |name: &str| proposals.iter().find(|p| p.name == name).unwrap();

    let quiet_one = named("changes/quiet");
    assert_eq!(quiet_one.title, "Say what to read");
    assert_eq!(quiet_one.revisions, [quiet.to_string()]);
    assert!(quiet_one.approvals.is_empty());

    let open = named("changes/explain");
    assert_eq!(open.title, "Explain how tasks are kept");
    assert_eq!(open.tip, tip.to_string());
    assert_eq!(open.revisions, [tip.to_string(), explain.to_string()]);
    let mut approvals: Vec<(&str, bool, Option<&str>)> = open
        .approvals
        .iter()
        .map(|a| (a.role.as_str(), a.verified, a.who.as_deref()))
        .collect();
    approvals.sort();
    assert_eq!(
        approvals,
        [
            ("author", true, None),
            ("reviewer", false, None),
            ("reviewer", true, Some("Grace Hopper <grace@example.com>")),
        ]
    );
    let by_grace = open
        .approvals
        .iter()
        .find(|a| a.who.is_some())
        .expect("Grace's");
    assert_eq!(by_grace.key, trusted_key.as_str());
    assert_eq!(by_grace.file, None);
}

#[test]
fn without_another_bookmark_the_heads_are_the_accepted_line() {
    let base = scratch("heads").join("repo");
    let mut store = Store::init(base.join("history")).expect("a store");
    write(&base, "README.md", "# Example\n");
    let begin = record_on(&base, &mut store, &[], "Begin", &[]);
    write(&base, "README.md", "# Example\n\nMore.\n");
    let onward = record_on(&base, &mut store, &[begin], "Go on", &[]);
    write(&base, "IDEA.md", "# An idea\n");
    let idea = record_on(&base, &mut store, &[begin], "Have an idea", &[]);
    bookmark(&mut store, "changes/idea", idea, false);

    let view = View::open(into_memory(&base.join("history"))).expect("the store opens");
    let proposals = view.proposals().expect("the proposals");
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].title, "Have an idea");
    assert_eq!(proposals[0].revisions, [idea.to_string()]);
    let _ = onward;
}
