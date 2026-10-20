---
title: A forge of forks and remarks
description: The hub grows into a forge in which nobody writes into anybody else's store — a contribution is a fork, and every issue, review, suggestion and offer is a remark in its writer's own chain — so the hub is a courier and an index, and never the authority
status: draft
created: 2026-10-19
updated: 2026-10-19
---

# A forge of forks and remarks

## Status

Draft. Adam made these choices:

- The forge grows out of [`hub/`](../../hub/README.md).
- A contribution is a fork, never a push into the maintainer's store.
- A discussion is remarks, not prov documents. Prov leaves the forge
  entirely.
- An account holds a minisign key per device, as GitHub holds SSH keys.
- An offer between forks appears on the repository's page once someone
  remarks on it there.
- A reporter can close their own issue.
- Private repositories are in scope, read with the device keys.
- The hub's spike layout at `/<slug>` is dropped, not migrated.

The open questions at the end are not decided.

## What is here already

Most of a forge exists, in four places, and none of them knows about the
others yet.

- **The page and the hub** (this repository) read a published store: README,
  files and their timelines, the revision graph, and the tracker that prov's
  tasks preset keeps. The hub serves that from Cloudflare, with accounts and
  push keys. A repository is `/<slug>` and belongs to whoever first pushed it.
- **historica** publishes and takes copies: `export` and `offer` write what a
  stranger fetches ([0042], [0052]), `fetch` takes it over HTTP, and
  `receive` unions it. [0086], built on main, makes a fetched stranger's line
  *held* — shown, never swept in — until somebody takes it with `carry`.
  Which line a copy stands on is a `Following` rule the embedder supplies,
  and `Following::Rule` takes any test of one revision.
- **historica-minisign** signs claims over a revision's digest ([0046]). A
  claim is a fact and travels. Trust in a key is an opinion and stays home.
- **historica-remark** is the Web Annotation model as a metadata block: a
  target, a quoted passage, a motivation, a body. `editing` is a patch. A
  *layer* is somebody else's single-writer chain of remarks, addressed to a
  store, kept beside it, and carried with it. The keeper can suppress a path
  but cannot rewrite it.

[0042]: https://github.com/diaryx-org/historica/blob/main/docs/decisions/0042-a-copy-to-take-away.md
[0046]: https://github.com/diaryx-org/historica/blob/main/docs/decisions/0046-who-vouches-for-a-revision.md
[0052]: https://github.com/diaryx-org/historica/blob/main/docs/decisions/0052-the-copy-a-stranger-fetches-from.md
[0057]: https://github.com/diaryx-org/historica/blob/main/docs/decisions/0057-the-stack-a-fetch-rides-on.md
[0072]: https://github.com/diaryx-org/historica/blob/main/docs/decisions/0072-a-command-this-tool-does-not-have.md
[0086]: https://github.com/diaryx-org/historica/blob/main/docs/decisions/0086-a-copy-chooses-its-heads.md

## The rule

**Nobody writes into a store they do not own.** Every piece of a forge that
GitHub keeps as a row in its own database is, here, a file in somebody's own
chain:

| GitHub | Here | Whose chain |
|---|---|---|
| repository | a published export | the owner's |
| fork | a published export sharing the repository's root | the contributor's |
| pull request | an `offering` remark naming a fork and a tip | the contributor's remarks |
| issue | a remark aimed at the repository, or a passage in it | the reporter's remarks |
| comment | a `replying` remark | the commenter's remarks |
| line comment | a remark quoting a passage at the offered tip | the reviewer's remarks |
| suggested change | an `editing` remark | the reviewer's remarks |
| approval | a minisign claim over the tip, role `reviewer` | the reviewer's claims |
| merge | `carry`, then the owner publishes | the owner's |
| close, reopen | a reply from the owner or the reporter | the closer's remarks |
| label | a reply from the owner | the owner's remarks |
| hide, moderate | suppression of a kept layer | the owner's copy only |

What the hub holds that is not one of these is an index, and the index can be
deleted and rebuilt from the chains. That is historica's own rule — *the
readable files are the authority* — applied to the forge.

## A fork

A fork is an export whose history shares the repository's root revision. On
the hub, forking is a copy the worker makes, under the contributor's name. A
fork anywhere else is any export at a URL.

The contributor works in their copy as in any other, and publishes it as
they would publish anything. Nothing they do reaches the owner's store.

## An offer

An offer is a remark pointing to a fork. It sits in the contributor's own
chain, with the motivation `offering`. Its body is the description. Its
fields name the fork's `offer.txt` and the tip being offered.

Two things say where an offer goes, and they are different. The *target* is
the project: the root every fork shares (see below). The *address* is whose
copy the offer asks to be taken into. A layer is already addressed to one
store, so an offer to the repository and an offer to somebody's fork are the
same remark in two differently addressed chains.

An offer between forks shows on the fork's page. It shows on the
repository's page once somebody remarks on it in a chain addressed to the
repository, so the repository sees the offers someone has brought to it,
and not every exchange between forks. Amending the offer is a new
revision of that remark. Withdrawing it is the contributor forgetting or
abandoning it.

The owner takes an offer with tools that exist:

```console
$ historica fetch https://hub.example/ada/historica/offer.txt
$ historica log                      # the offered tip shows as held (0086)
$ historica carry <tip> --onto <head>
```

and then publishes. The offer is closed by the owner's reply, or by the hub
seeing the offered changes in the owner's line.

The owner's line is the heads of the published store under a `Following`
rule that accepts what the owner's keys vouched for. That is the rule [0086]
names for a forge, through historica-minisign. A take names the offered tip
on `supersedes`, so "the owner's line took this offer" is a question about
the owner's heads.

This repository's History page already lists *proposals*: shared bookmarks
`changes/<id>` that the accepted line does not reach. That is the in-store
proposal Diaryx's change sets write, and it stays, under the name it has. An
offer is the stranger's equivalent, and arrives from outside the store.

## An issue, and every other discussion

An issue is a remark aimed at the repository, with the motivation
`questioning` or `commenting`, or aimed at a passage in a file when the
report is about that passage. A thread is the `replying` remarks under it.
A reporter does not need a fork: they need a chain of remarks, and on the hub
every account has one per repository it remarks on.

The owner keeps the layers they choose to keep. A kept layer is carried with
the repository, so the discussion the owner kept travels with the code, and a
copy taken years later still holds it, under its writers' own signatures. A
layer the owner has not kept is still visible on the hub, marked as not kept.
Keeping a layer is how a newcomer becomes a participant.

An issue is closed by a closing reply from the owner or from the reporter,
the writer of the root remark. Either can reopen it, and the latest of their
replies wins. Labels are the owner's replies. The hub folds the thread into
one state, and the fold is a function of the remarks, so anybody holding the
same remarks gets the same state.

The owner, for this purpose, is whoever's keys the reader trusts for the
repository. On the hub, that is the keys the owner's account has registered.
A reporter is recognised by the same means: the key that signed the root
remark.

A layer the owner has not kept is shown to every reader, folded away under
the thread by default and marked as not kept. Hiding it from everyone but
the owner would protect nobody, because the reporter's chain is published at
its own URL whatever the repository's page shows. What hiding *would* do is
make the hub the judge of who may speak, which this design exists to avoid.
Spam is answered by the owner's suppression on their own page, by the hub
refusing an account, and by limits on how fast a new account can write.

## What a remark targets

A target is an opaque string to historica-remark, and the forge has to choose
one. The proposal is that a repository is named by its root revision's
digest, and a file by that and the file's identifier. Historica files carry
identifiers that survive a rename ([0008]), and every fork shares the root,
so one target names the repository on every host and in every fork, and a
remark on a file follows it through moves. `at` names the revision the writer
saw, which historica-remark already carries.

[0008]: https://github.com/diaryx-org/historica/blob/main/docs/decisions/0008-tree.md

## Keys

An account holds a minisign key for each device, the way a GitHub account
holds SSH keys. A secret key never leaves the device it was made on, so
nothing has to carry one.

The page can make a key for the browser it runs in, so the secret part never
reaches the hub. It shows the secret key once, for the person to keep, and
keeps it in the browser's own storage so the page can sign the remarks and
claims written there. A person can also add a public key they made
themselves, proved by signing a challenge with it, for the command line. The
hub keeps only public keys.

Each key keeps when it was added and when it was revoked. A claim made by a
revoked key still checks for what it signed before revocation. A key that
leaked is revoked as of a date the person names, and claims after it no
longer count. Every key on the account counts as the account: the owner's
line accepts what any of the owner's keys vouched for.

## The hub

The hub stays what its README says, a courier with pages, and gains:

- **Owners in the path.** A repository is `/<owner>/<slug>`, and a fork is
  `/<contributor>/<slug>`. Today's `/<slug>` and first-push ownership move
  under the owner's name.
- **One copy of every payload.** A fork shares almost every file with its
  repository. R2 keeps a payload once, under its digest, and the worker
  serves `/<owner>/<slug>/<path>` by looking the path up in that repository's
  manifest. A fork costs its manifest and what it changed.
- **Remark chains per account.** Each account publishes, per repository it
  remarks on, a chain at `/<account>/<slug>/remarks/`. The hub writes these on
  the account's behalf from a form, or takes them pushed.
- **An index.** Forks are found by their shared root. Offers, issues and
  replies are found by target, in every chain the hub holds and every chain a
  person has registered by URL. The index is a cache, and rebuilding it
  replays the chains.
- **Pages.** A repository gains **Offers** (open and closed, each with its
  revisions against the owner's line, its review remarks, its claims, and the
  commands to take it) and **Issues** (threads, with kept and not-kept
  marked). Every page has a **Fork** button, and each passage on the Files
  page can be remarked on.
- **No Tasks page.** The prov tracker and this repository's `prov` feature
  go. A repository that keeps prov documents shows them as files.
- **Private repositories.** Every read of a private repository goes through
  the worker, which checks the reader against the repository's members.
  Its forks are private to the same members and the fork's owner, and so is
  every remark chain addressed to it. Nothing private is served as a plain
  file, and the static view is built per request or behind the same check.
- **Reading with a device key.** historica's fetch has no authentication
  ([0057] defers it to the transport), and the device keys above are the
  answer. `historica minisign fetch <url>` (a command of historica-minisign,
  reached through historica's [0072]) fetches through a source that signs
  each request: `Authorization: Minisign <key-id> <time> <signature>`, over
  the method, host, path and time. The hub accepts a key registered to a
  member, within a few minutes of the time it states. A copied header reads
  one path, for minutes. The page signs with the key it keeps. `git clone`
  cannot sign, so historica-git's credential helper signs a challenge for a
  short-lived token, and the hub takes the token as a Basic password.

## What each repository builds

The work, in roughly the order it can land. None of it is a deploy.

**historica-remark**

1. An `offering` motivation: a body, the fork's manifest URL, and the tip.
   The Web Annotation model allows motivations of its own beside its
   thirteen.
2. A closing reply, and labels, as fields a `replying` remark may carry, and
   a fold that turns a thread into its state given whose replies count.

**historica-minisign**

3. A `Following` rule: accept a revision that a claim, made by a key the
   rule is given and valid at the claim's time, vouches for.
4. A key's history: the keys an identity has had, and when each was retired,
   read by that rule.

**historica-view (the page and the hub)**

5. Targets: name a repository by its root revision and a file by its
   identifier, and resolve one back in a store.
6. Owners in the path, and a fork as a copy under the contributor's name.
7. Payloads kept once by digest in R2.
8. Remark chains per account per repository, written and signed in the page.
9. The index, rebuilt from chains.
10. Issues and Offers pages, and remarks on a passage from the Files page.
11. Drop the Tasks page and the `prov` feature.
12. Keys per device: made in the browser and shown once, or added by public
    key; revoked as of a date; signing in the page.
13. The owner's line from the minisign rule, in place of the shared-bookmark
    accepted line.
14. Private repositories: members, gated reads, gated forks and chains.
15. Signed reads: check the `Minisign` header, and mint short-lived tokens
    for a signed challenge.

**historica-minisign**, again

16. Request signing, a signing `Source`, `fetch`, and the token client.

**historica-git**

17. A git credential helper that gets a token through historica-minisign.

**historica**

Nothing found yet. As of main (`8ae0f57`), fetching a fork makes its tip
held, `log` marks it, `carry` takes it, and `Following::Rule` is where the
minisign rule plugs in.

## Open questions

1. **A reporter without an account.** Off the hub, anyone with a chain and a
   key can remark. Does the hub index a chain registered by URL from someone
   with no account, and under what limits?

## Not in this proposal

- **CI** on an offer: the hub could run `ship ci` on a fork's tip, but where
  it runs is its own decision.
- **Notifications.**
- **Moving the org from tasks and proposals to remarks.** Adam is weighing
  that separately, with Diaryx in the picture. This proposal is written in
  the org's present convention, and would move with the rest.
