//! A history recorded on disk by the library, read back through the
//! in-memory folder the page uses.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use historica::core::RevisionId;
use historica::record::{Clock as _, Platform, Recording, Restriction, record};
use historica::store::Store;
use historica::working::Working;
use historica_view::memory::Memory;
use historica_view::view::{ROOT, View};

const AUTHOR: &str = "Ada Lovelace <ada@example.com>";

fn repository(test: &str) -> (PathBuf, Store) {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = fs::remove_dir_all(&root);
    let base = root.join("repo");
    fs::create_dir_all(&base).expect("a folder");
    let store = Store::init(base.join("history")).expect("a store");
    (base, store)
}

fn record_folder(
    base: &Path,
    store: &mut Store,
    parents: Vec<RevisionId>,
    message: &str,
) -> RevisionId {
    let working = Working::read(base, store.skipped()).expect("the folder");
    let recording = Recording {
        parents,
        author: AUTHOR.to_owned(),
        when: Platform.now().expect("a clock"),
        message: message.to_owned(),
        moves: Vec::new(),
        at: Vec::new(),
        accepted: BTreeSet::new(),
        only: Restriction::Everything,
        kinds: Default::default(),
        extensions: Default::default(),
    };
    record(store, &working, &recording, &mut Platform)
        .expect("recording")
        .revision
}

/// Every file under `directory`, placed under [`ROOT`] as the page places them.
fn into_memory(directory: &Path) -> Memory {
    fn walk(memory: &mut Memory, root: &Path, at: &Path) {
        for entry in fs::read_dir(at).expect("a directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(memory, root, &path);
            } else {
                let relative = path.strip_prefix(root).expect("beneath the root");
                memory.insert(
                    Path::new(ROOT).join(relative),
                    fs::read(&path).expect("a file"),
                );
            }
        }
    }
    let mut memory = Memory::new();
    walk(&mut memory, directory, directory);
    memory
}

/// A root, two branches from it, and the merge of both.
fn diamond(test: &str) -> (PathBuf, [RevisionId; 4]) {
    let (base, mut store) = repository(test);
    fs::write(base.join("poem.txt"), "one\ntwo\nthree\n").unwrap();
    let root = record_folder(&base, &mut store, vec![], "Begin the poem");

    fs::write(base.join("poem.txt"), "one\n2\nthree\n").unwrap();
    let left = record_folder(&base, &mut store, vec![root], "Spell two as a digit");

    fs::write(base.join("poem.txt"), "one\ntwo\nthree\n").unwrap();
    fs::write(base.join("notes.txt"), "a note\n").unwrap();
    let right = record_folder(&base, &mut store, vec![root], "Add notes");

    fs::write(base.join("poem.txt"), "one\n2\nthree\n").unwrap();
    let merge = record_folder(&base, &mut store, vec![left, right], "Merge the notes");
    (base, [root, left, right, merge])
}

#[test]
fn the_graph_puts_every_child_above_its_parents() {
    let (base, [root, left, right, merge]) = diamond("graph");
    let view = View::open(into_memory(&base.join("history"))).expect("the store opens");
    let graph = view.graph();

    let order: Vec<&str> = graph.revisions.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(order.len(), 4);
    assert_eq!(order[0], merge.to_string());
    assert_eq!(order[3], root.to_string());
    assert!(order[1..3].contains(&left.to_string().as_str()));
    assert!(order[1..3].contains(&right.to_string().as_str()));

    let heads: Vec<&str> = graph
        .revisions
        .iter()
        .filter(|r| r.head)
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(heads, [merge.to_string()]);
    assert_eq!(graph.revisions[0].parents.len(), 2);
    assert_eq!(graph.revisions[0].message.trim(), "Merge the notes");
}

#[test]
fn a_revision_shows_the_lines_it_changed() {
    let (base, [root, left, right, _]) = diamond("detail");
    let view = View::open(into_memory(&base.join("history"))).expect("the store opens");

    let detail = view.revision(&left.to_string()).expect("the revision");
    assert_eq!(detail.against, Some(root.to_string()));
    assert_eq!(detail.files.len(), 1);
    let poem = &detail.files[0];
    assert_eq!(poem.kind, "edited");
    let signed: Vec<(char, &str)> = poem.hunks.as_ref().unwrap()[0]
        .lines
        .iter()
        .map(|line| (line.sign, line.text.as_str()))
        .collect();
    assert_eq!(
        signed,
        [(' ', "one"), ('-', "two"), ('+', "2"), (' ', "three")]
    );

    let added = view.revision(&right.to_string()).expect("the revision");
    assert_eq!(added.files.len(), 1);
    assert_eq!(added.files[0].kind, "added");
    assert_eq!(added.files[0].to.as_deref(), Some("notes.txt"));

    let first = view.revision(&root.to_string()).expect("the root");
    assert_eq!(first.against, None);
    assert_eq!(first.files[0].kind, "added");
}
