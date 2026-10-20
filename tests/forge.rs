//! A repository with a README, recorded, published as a stranger finds it,
//! and read back through the manifest the way the page reads it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use historica::core::RevisionId;
use historica::record::{Clock as _, Platform, Recording, Restriction, record};
use historica::store::Store;
use historica::working::Working;
use historica_view::memory::Memory;
use historica_view::published::{Published, verified};
use historica_view::view::{ROOT, View};

const AUTHOR: &str = "Ada Lovelace <ada@example.com>";

fn scratch(test: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("forge-{test}"));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("a folder");
    root
}

fn record_folder(base: &Path, store: &mut Store, message: &str) -> RevisionId {
    let working = Working::read(base, store.skipped()).expect("the folder");
    let recording = Recording {
        parents: store.history().heads().into_iter().collect(),
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

fn write(base: &Path, path: &str, text: &str) {
    let path = base.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A README with frontmatter, and a prov tracker beside it, which the view
/// reads as nothing but files.
fn workspace(base: &Path) -> [RevisionId; 2] {
    let mut store = Store::init(base.join("history")).expect("a store");
    write(
        base,
        "README.md",
        "---\ntitle: Example\ndescription: A repository with a tracker\ncontents:\n- '[Tasks](/docs/tasks/tasks.md)'\n---\n# Example\n\nSee [the tasks](docs/tasks/tasks.md).\n",
    );
    write(
        base,
        "docs/tasks/tasks.md",
        "---\ntitle: Tasks\npart_of: '[Example](/README.md)'\ncontents:\n- '[a merge agrees by text](/docs/tasks/a-merge-agrees-by-text.md)'\n---\n# Tasks\n",
    );
    write(
        base,
        "docs/tasks/a-merge-agrees-by-text.md",
        "---\ntitle: a merge agrees by text\npart_of: '[Tasks](/docs/tasks/tasks.md)'\nstatus: open\n---\n# a merge agrees by text\n",
    );
    let first = record_folder(base, &mut store, "Begin the tracker");

    write(
        base,
        "docs/tasks/a-merge-agrees-by-text.md",
        "---\ntitle: a merge agrees by text\npart_of: '[Tasks](/docs/tasks/tasks.md)'\nstatus: done\n---\n# a merge agrees by text\n",
    );
    let second = record_folder(base, &mut store, "Close the task");
    [first, second]
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

#[test]
fn the_front_page_is_the_readme_and_a_tracker_is_only_files() {
    let root = scratch("overview");
    let base = root.join("repo");
    let [_, second] = workspace(&base);
    let view = View::open(into_memory(&base.join("history"))).expect("the store opens");

    let overview = view.overview(&second.to_string()).expect("the overview");
    assert_eq!(overview.title.as_deref(), Some("Example"));
    assert_eq!(
        overview.description.as_deref(),
        Some("A repository with a tracker")
    );
    let readme = overview.readme.expect("a README");
    assert_eq!(readme.path, "README.md");
    assert!(readme.html.contains("<h1>Example</h1>"), "{}", readme.html);
    assert!(!readme.html.contains("description:"), "{}", readme.html);

    let paths: Vec<String> = view
        .files(&second.to_string())
        .unwrap()
        .into_iter()
        .map(|file| file.path)
        .collect();
    assert_eq!(
        paths,
        [
            "README.md",
            "docs/tasks/a-merge-agrees-by-text.md",
            "docs/tasks/tasks.md"
        ]
    );
}

#[test]
fn a_published_copy_reads_back_through_its_paged_manifest() {
    let root = scratch("published");
    let base = root.join("repo");
    let [_, second] = workspace(&base);
    let store = Store::open(base.join("history")).expect("the store");

    // Decision 0052's shape: an export, and a manifest beside it.
    let site = root.join("site");
    store
        .export(site.join("store"), &second)
        .expect("the export");
    let copy = Store::open(site.join("store/history")).expect("the copy");
    copy.publish_offer("store", &site.join("offer.txt"))
        .expect("the manifest");

    // What the page does: read the manifest, fetch its pages, then fetch and
    // check every file it lists.
    let text = fs::read_to_string(site.join("offer.txt")).unwrap();
    let mut published = Published::read(&text).expect("a manifest");
    let wanted = published.wanted();
    assert!(!wanted.is_empty(), "a paged manifest names its pages");
    for (path, digest) in published.pages() {
        assert!(wanted.contains(&path));
        assert!(verified(&fs::read(site.join(&path)).unwrap(), &digest));
    }
    for page in wanted {
        let bytes = fs::read(site.join(&page)).unwrap();
        published.page(&page, &bytes).expect("the page it named");
    }
    let listing = published.listing().expect("the listing");
    assert_eq!(listing.header, "store/history/historica.txt");

    let mut memory = Memory::new();
    memory.insert(
        Path::new(ROOT).join("historica.txt"),
        fs::read(site.join(&listing.header)).unwrap(),
    );
    for file in &listing.files {
        let bytes = fs::read(site.join(&file.path)).unwrap();
        assert!(
            verified(&bytes, &file.digest),
            "{} is what it says",
            file.path
        );
        memory.insert(Path::new(ROOT).join(&file.store), bytes);
    }
    let view = View::open(memory).expect("the copy opens");
    let graph = view.graph();
    assert_eq!(graph.revisions.len(), 2);
    assert_eq!(graph.revisions[0].id, second.to_string());
    assert_eq!(view.files(&second.to_string()).unwrap().len(), 3);
}

#[test]
fn a_readme_without_markdown_is_still_the_front_page() {
    let root = scratch("plain");
    let base = root.join("repo");
    let mut store = Store::init(base.join("history")).expect("a store");
    write(&base, "README", "Plain text, and no frontmatter.\n");
    let at = record_folder(&base, &mut store, "Begin").to_string();
    let view = View::open(into_memory(&base.join("history"))).expect("the store opens");

    assert_eq!(view.graph().revisions.len(), 1);
    let overview = view.overview(&at).expect("the overview");
    assert_eq!(overview.title, None);
    assert_eq!(overview.readme.expect("a README").path, "README");
}

#[test]
fn a_page_that_is_not_the_one_named_is_refused() {
    let root = scratch("tampered");
    let base = root.join("repo");
    let [_, second] = workspace(&base);
    let store = Store::open(base.join("history")).expect("the store");
    let site = root.join("site");
    store.export(site.join("store"), &second).unwrap();
    let copy = Store::open(site.join("store/history")).unwrap();
    copy.publish_offer("store", &site.join("offer.txt"))
        .unwrap();

    let text = fs::read_to_string(site.join("offer.txt")).unwrap();
    let mut published = Published::read(&text).unwrap();
    let page = published.wanted().remove(0);
    let mut bytes = fs::read(site.join(&page)).unwrap();
    bytes.extend_from_slice(b"revision 0000 - store/history/revisions/extra.rev.txt\n");
    assert!(published.page(&page, &bytes).is_err());
    assert!(published.listing().is_err(), "nothing is listed without it");
}
