//! Targets made from a recorded store, and resolved back in it and in others.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use historica::core::RevisionId;
use historica::record::{Clock as _, Platform, Recording, Restriction, record};
use historica::store::Store;
use historica::working::Working;
use historica_view::memory::Memory;
use historica_view::target::Target;
use historica_view::view::{ROOT, View};

const AUTHOR: &str = "Ada Lovelace <ada@example.com>";

fn repository(test: &str) -> (PathBuf, Store) {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("targets-{test}"));
    let _ = fs::remove_dir_all(&root);
    let base = root.join("repo");
    fs::create_dir_all(&base).expect("a folder");
    let store = Store::init(base.join("history")).expect("a store");
    (base, store)
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

fn record_folder(
    base: &Path,
    store: &mut Store,
    parents: Vec<RevisionId>,
    message: &str,
) -> RevisionId {
    record_moving(base, store, parents, message, Vec::new())
}

fn open(base: &Path) -> View {
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
    let directory = base.join("history");
    let mut memory = Memory::new();
    walk(&mut memory, &directory, &directory);
    View::open(memory).expect("the store opens")
}

#[test]
fn a_file_target_follows_the_file_through_a_move() {
    let (base, mut store) = repository("moves");
    fs::write(base.join("notes.txt"), "first\n").unwrap();
    let added = record_folder(&base, &mut store, vec![], "Add notes");
    fs::create_dir(base.join("docs")).unwrap();
    fs::rename(base.join("notes.txt"), base.join("docs/notes.txt")).unwrap();
    let moved = record_moving(
        &base,
        &mut store,
        vec![added],
        "File the notes",
        vec![("notes.txt".into(), "docs/notes.txt".into())],
    );
    fs::write(base.join("other.txt"), "other\n").unwrap();
    fs::remove_file(base.join("docs/notes.txt")).unwrap();
    let dropped = record_folder(&base, &mut store, vec![moved], "Drop the notes");
    let view = open(&base);

    let notes = view.files(&added.to_string()).unwrap().remove(0).file;
    let target = view.file_target(&notes).expect("a target");
    assert_eq!(target.root, added, "the first revision is the root");
    assert_eq!(target.to_string(), format!("historica:{added}/{notes}"));

    // What a remark carries comes back as the same file wherever it is.
    let carried: Target = target.to_string().parse().expect("its own spelling");
    let path = |at: RevisionId| view.target_path(&carried, &at.to_string()).unwrap();
    assert_eq!(path(added).as_deref(), Some("notes.txt"));
    assert_eq!(path(moved).as_deref(), Some("docs/notes.txt"));
    assert_eq!(path(dropped), None);

    // A repository target names no file.
    assert!(
        view.target_path(&target.in_repository(), &moved.to_string())
            .is_err()
    );
}

#[test]
fn a_store_answers_to_its_own_root_and_not_to_another() {
    let (base, mut store) = repository("ours");
    fs::write(base.join("poem.txt"), "one\n").unwrap();
    let root = record_folder(&base, &mut store, vec![], "Begin");
    fs::write(base.join("poem.txt"), "one\ntwo\n").unwrap();
    let later = record_folder(&base, &mut store, vec![root], "Go on");
    let ours = open(&base);

    let (elsewhere, mut theirs) = repository("theirs");
    fs::write(elsewhere.join("poem.txt"), "one\n").unwrap();
    let their_root = record_folder(&elsewhere, &mut theirs, vec![], "Begin elsewhere");
    let theirs = open(&elsewhere);

    let target = ours.target().expect("a repository");
    assert_eq!(target, Target::repository(root));
    assert!(ours.answers_to(&target));
    assert!(!theirs.answers_to(&target));
    assert!(!ours.answers_to(&Target::repository(their_root)));
    // A revision the store holds that is not a root is not a repository.
    assert!(!ours.answers_to(&Target::repository(later)));

    // A file target in another repository is refused, not looked up.
    let poem = ours.files(&later.to_string()).unwrap().remove(0).file;
    let foreign = Target::file(their_root, poem.parse().unwrap());
    assert!(ours.target_path(&foreign, &later.to_string()).is_err());
}

#[test]
fn a_store_joined_from_two_histories_answers_to_both_roots() {
    let (base, mut store) = repository("joined");
    fs::write(base.join("left.txt"), "left\n").unwrap();
    let left = record_folder(&base, &mut store, vec![], "Begin on the left");
    fs::remove_file(base.join("left.txt")).unwrap();
    fs::write(base.join("right.txt"), "right\n").unwrap();
    let right = record_folder(&base, &mut store, vec![], "Begin on the right");
    fs::write(base.join("left.txt"), "left\n").unwrap();
    record_folder(&base, &mut store, vec![left, right], "Join them");
    let view = open(&base);

    assert_eq!(view.roots(), BTreeSet::from([left, right]));
    assert!(view.answers_to(&Target::repository(left)));
    assert!(view.answers_to(&Target::repository(right)));
    // The name is the lower digest of the two, whichever was recorded first.
    assert_eq!(view.target(), Some(Target::repository(left.min(right))));
}
