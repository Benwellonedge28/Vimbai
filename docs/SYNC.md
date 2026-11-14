# Offline and sync

The app is offline-first: a person works on a plane and the work arrives. This is what makes that true on the server
side, and what it refuses to make true.

## The three processes involved

| Process | What it is for |
|---|---|
| `apps/flutter` | the device. Holds a Book's entries in a local database, queues what it changed, pushes on reconnect |
| `apps/sync-server` (`vimbai-sync`) | the Book surface alone: sign in, push, pull, devices, membership |
| `apps/api-server` (the engine) | every address, of which the sync server serves a handful |

The sync server is not a second implementation. It is the engine's own router with a narrower door in front of it: the
push that deduplicates an entry id, the pull that walks a sequence, the membership check that decides who may read, are
the same functions the cluster serves. What it adds is a deployment shape, because sync is the one thing that has to scale
with the number of devices rather than the number of books.

Its surface is `/health`, `/identity` and `/books`. Everything else answers 404 and says so. A sync pod that will post a
journal entry is a sync pod that can be used to post one.

## The sequence, and why a device quotes it back

Every entry stored in a Book takes the next number in that Book's sequence, and the Book records how far it has got. A
device pulls with `?since=N` and is answered with the entries after `N` *and* the Book's current high-water mark
(`book_seq`), which is what it quotes next time. That is why paging a listing has to keep the handler's own fields beside
its rows: an envelope holding only rows would leave a device holding data it could not place.

## Pushing twice

A push is retry-safe by entry id. A device whose answer was lost resends the same batch and is told `already_stored`, so
nothing double-posts and the device learns its batch arrived the first time. A batch that was interrupted halfway resumes
the same way: the device cannot know which entries landed, so it sends the whole queue and the server counts what it
already held.

## When two devices changed the same record

An entry is an opaque, already-encrypted operation, so the server cannot read what it changes - the Book's key never
reaches it. "Cannot see the change" is not the same as "cannot see a collision", and the difference matters most where it
is least visible: two people editing the same journal entry on two devices, both offline, both syncing the same morning.

So an entry may **declare** what it was made against:

```
{"entry_id": "...", "payload": "...", "record": {"key": "journal_entry:42", "version": 3}}
```

The key is opaque to the server, a name the client chooses. The version is the one its author had in front of them. An
entry that declares nothing is an append-only event and can never conflict, which is what most entries are, so a client
that does not use this is unaffected.

The rule is refusal by default:

* the version it names is the version the Book holds - accept it, and the record moves on one;
* the Book has moved past it and the record is a family where losing an update costs nothing worth refusing a sync over
  (`note`, `draft`, `preference`, `view`, `setting`, `template`) - accept it, last writer wins, and say so in the answer;
* otherwise - **refuse**, and say which record, which version the entry was made against and which version the Book is at:

```
409 Conflict
journal_entry:42 was changed by another device: this entry was made against version 0, the book is at 1 (a1)
```

Accounting families are not on that list and are not meant to be. A lost ledger update is a wrong balance, and a shared
ledger entry is immutable, so a correction is a new entry rather than an edit of an old one.

The whole batch is decided before any of it is written, so a refusal stores nothing and a device is never left
half-synced. That is also what the list of accepted records in the answer is for: a device that gets one is told exactly
which records moved.

## What is tested, and against what

Five tests in `rust/apps/sync-server/tests/sync_scenarios.rs` start the real binary and speak HTTP to it:

1. a device that was away syncs once, not twice - the repeat is answered `accepted: 0, already_stored: 3` and the Book
   holds three entries, not six;
2. two devices that changed the same ledger entry: the second is refused **409**, the reason names the record and both
   versions, and the refused batch stored nothing;
3. a note is last-write-wins and the answer says so;
4. an interrupted batch resumes - five entries after three landed gives `accepted: 2, already_stored: 3` and a Book of
   five in order;
5. a sync server will not post a journal entry.

The conflict rules themselves have six unit tests in `rust/crates/platform/offline-sync/src/conflict.rs`, including one
that holds a batch changing the same record twice (a create and then an edit) as a device catching up with itself rather
than a conflict.
