---
title: A retargeted link is left out of its revision
part_of: '[Tasks](tasks.md)'
created: 2026-11-08T13:45:39.956366Z
status: done
description: '`View::revision` reads both sides of a link as `Side::Other(None)`, so a revision that only points a link somewhere else compares its two sides as equal and lists no change to the file, while `file_history` lists the same revision as an edit of it'
---

# A retargeted link is left out of its revision

**Done** in `fix(view): list a retargeted link as a change of its revision`.
`Side` has a `Link` variant carrying the tree entry's target, compared as
historica's `LinkTarget`, so a link to a file that only moved is unchanged;
the repro is `a_retargeted_link_is_a_change_of_its_revision` in
`tests/view.rs`.

[`View::revision`](../../src/view.rs) lists what a revision did to each file,
and leaves a file out where its two sides compare equal and it neither moved
nor changed mode. `View::text` gives a link `Side::Other(None)` on either
side, whatever it points at, so two different targets compare equal. A
revision that only retargets a link therefore shows nothing for it in the
History view.

The rest of the view knows the revision changed the file. `touches` counts
`document.links` as `edited`, so `file_history` lists the revision as a
version of the link, and `file_at` gives each revision's own `target`.
Picking that version in Files and then opening the revision shows a revision
that, as far as the page says, changed nothing.

Found at 1544962.

## Repro

In `tests/view.rs`, beside its helpers:

```rust
#[test]
fn a_retargeted_link_is_a_change_of_its_revision() {
    let (base, mut store) = repository("retargeted-link");
    fs::write(base.join("a.txt"), "a\n").unwrap();
    fs::write(base.join("b.txt"), "b\n").unwrap();
    std::os::unix::fs::symlink("a.txt", base.join("current")).unwrap();
    let root = record_folder(&base, &mut store, vec![], "Point at a");
    fs::remove_file(base.join("current")).unwrap();
    std::os::unix::fs::symlink("b.txt", base.join("current")).unwrap();
    let next = record_folder(&base, &mut store, vec![root], "Point at b");

    let view = View::open(into_memory(&base.join("history"))).expect("the store opens");
    let detail = view.revision(&next.to_string(), None).unwrap();
    let files: Vec<_> = detail
        .files
        .iter()
        .map(|change| (change.to.as_deref(), change.kind, change.hunks.is_none()))
        .collect();
    assert_eq!(files, [(Some("current"), "edited", true)]); // fails: files is empty
}
```

`file_at` on the two revisions gives targets `a.txt` and `b.txt`, and
`file_history` gives `[edited, added]`: the store holds the change, and only
`revision` drops it.

## Done when

`View::revision` lists a link whose target changed as `edited`, with no
hunks, as it does a file of bytes whose payload changed, and the repro above
is in `tests/view.rs` and passes. Comparing the targets is enough: giving
`Side` a link variant that carries the tree entry's target, and treating it
as `Other` for hunks, passed the repro and every existing test when tried.
