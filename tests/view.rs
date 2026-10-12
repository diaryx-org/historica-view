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
    record_moving(base, store, parents, message, Vec::new())
}

fn record_moving(
    base: &Path,
    store: &mut Store,
    parents: Vec<RevisionId>,
    message: &str,
    moves: Vec<(String, String)>,
) -> RevisionId {
    let working = Working::read(base, store.skipped()).expect("the folder");
    let recording = Recording {
        parents,
        author: AUTHOR.to_owned(),
        when: Platform.now().expect("a clock"),
        message: message.to_owned(),
        moves,
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

/// A file that is edited, moved into a folder, edited again, and a second
/// file beside it.
fn travelling(test: &str) -> (PathBuf, [RevisionId; 4]) {
    let (base, mut store) = repository(test);
    fs::write(base.join("notes.txt"), "first\n").unwrap();
    fs::write(base.join("other.txt"), "other\n").unwrap();
    let added = record_folder(&base, &mut store, vec![], "Add notes");

    fs::write(base.join("notes.txt"), "first\nsecond\n").unwrap();
    let edited = record_folder(&base, &mut store, vec![added], "Add a line");

    fs::create_dir(base.join("docs")).unwrap();
    fs::rename(base.join("notes.txt"), base.join("docs/notes.txt")).unwrap();
    let moved = record_moving(
        &base,
        &mut store,
        vec![edited],
        "File the notes",
        vec![("notes.txt".into(), "docs/notes.txt".into())],
    );

    fs::write(base.join("docs/notes.txt"), "first\nsecond\nthird\n").unwrap();
    let again = record_folder(&base, &mut store, vec![moved], "Add another line");
    (base, [added, edited, moved, again])
}

#[test]
fn a_file_keeps_its_history_through_a_move() {
    let (base, [added, edited, moved, again]) = travelling("history");
    let view = View::open(into_memory(&base.join("history"))).expect("the store opens");

    let files = view.files(&again.to_string()).unwrap();
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["docs/notes.txt", "other.txt"]);
    let notes = &files[0].file;

    let history = view.file_history(notes).unwrap();
    let seen: Vec<(String, &str, Option<&str>)> = history
        .iter()
        .map(|v| (v.revision.clone(), v.kind, v.path.as_deref()))
        .collect();
    assert_eq!(
        seen,
        [
            (again.to_string(), "edited", Some("docs/notes.txt")),
            (moved.to_string(), "moved", Some("docs/notes.txt")),
            (edited.to_string(), "edited", Some("notes.txt")),
            (added.to_string(), "added", Some("notes.txt")),
        ]
    );

    // Back in time, the same file is at its old path with its old content.
    let then = view.file_at(&added.to_string(), notes).unwrap().unwrap();
    assert_eq!(then.path, "notes.txt");
    assert_eq!(then.text.as_deref(), Some("first\n"));

    let hunks = view
        .file_diff(Some(&moved.to_string()), &again.to_string(), notes)
        .unwrap()
        .unwrap();
    let inserted: Vec<&str> = hunks[0]
        .lines
        .iter()
        .filter(|line| line.sign == '+')
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(inserted, ["third"]);
}

#[test]
fn a_folder_history_counts_the_files_it_holds_now() {
    let (base, [added, edited, moved, again]) = travelling("folder");
    let view = View::open(into_memory(&base.join("history"))).expect("the store opens");

    let docs = view.folder_history(&again.to_string(), "docs").unwrap();
    let revisions: Vec<String> = docs.iter().map(|v| v.revision.clone()).collect();
    assert_eq!(
        revisions,
        [again, moved, edited, added].map(|id| id.to_string())
    );

    let whole = view.folder_history(&again.to_string(), "").unwrap();
    assert_eq!(whole.last().unwrap().files, 2);
    // `doc` is a prefix of `docs` and not a folder that holds anything.
    assert!(
        view.folder_history(&again.to_string(), "doc")
            .unwrap()
            .is_empty()
    );
}
