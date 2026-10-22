---
title: A push can drop the published history
description: The hub takes any manifest whose files the repository holds, so a push of an unrelated or rewritten store replaces the published history and deletes its files, and the only guard is a cache on the pushing machine
status: open
created: 2026-10-21
updated: 2026-10-21
---

# A push can drop the published history

`put_offer` in `hub/worker/src/push.rs` checks that the repository holds every
digest a new manifest names, and then takes it. It never compares the new
history with the one it replaces. `db::settle` then releases every file the
new listings no longer name, and deletes the blobs no repository holds. The
git mirror goes the same way: the CLI writes it and runs `git push --mirror`,
which forces.

So a push is a force push. What stopped one by accident on 2026-10-21 was the
CLI's cache. `historica hub push` exports onto the copy it made last time,
under `~/Library/Caches/historica-hub/…`, and `historica export` refuses a copy
that holds revisions the store lacks ([historica's 0052]). That guard belongs
to one machine. A push from a second machine, or after the cache is cleared,
replaces the published history without a word.

## Repro

historica-git's store was rebuilt by a fresh import, which kept 46 revisions
and replaced 6.

```console
$ historica hub push --public
historica: …/site/historica-git holds 14fb6551471f, which this store does not: …
historica hub: historica export failed
$ mv ~/Library/Caches/historica-hub/…/adam/historica-git <elsewhere>
$ historica hub push --public
pushed … : 365 files sent
```

The second push replaced the history, and the 6 old revisions are gone from
the hub. A reader who fetched before has them, beside the new line, as a
second head.

## The work

- **The worker refuses a manifest that drops a tip it published.** The old
  manifest's `head` line names the tip, and the worker can already open a
  store (the `Repository` Durable Object opens `history.tar` with the
  historica library). A new store is accepted where every old tip is in it,
  or is superseded by a revision in it, which is a rewrite that says what it
  replaces. Otherwise `409`, naming the tips that would be dropped.
- **Replacing is said, not inferred.** `historica hub push --replace` sends a
  header that lets the worker take a manifest that drops tips. The CLI asks
  at a terminal, as it asks `--public` or `--private`, and refuses without a
  terminal unless the flag is given.
- **What a refusal leaves.** The blobs a refused push sent are held by the
  repository until the next manifest is taken or the next settle, as any
  push's blobs are today.

## Done when

- A push from a machine with no cache, of a store that lacks a published tip,
  is refused by the worker, and names the tips.
- The same push with `--replace` is taken, and the report says how many
  revisions the hub stopped serving.
- A push of a store that supersedes the published tip, as an `amend` or a
  `carry` does, is taken without `--replace`.

[historica's 0052]: https://github.com/diaryx-org/historica/blob/main/docs/decisions/0052-the-copy-a-stranger-fetches-from.md
