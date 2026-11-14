# Vimbai local runtime

_On-device, offline runtime._ The whole book behind one process on a machine with no server: no internet, no Docker, no
Kubernetes, no database. Data lives in a folder and survives restarts.

It is a Rust binary. It used to be a Python package that booted FastAPI service apps and started an engine beside itself;
every service it mounted is answered by the engine now, so what was a launcher became the runtime and the Python launcher
went with it.

## Quick start

```bash
cargo build --release -p vimbai_local        # in rust/, once

rust/target/release/vimbai-local                       # core bookkeeping profile, http://127.0.0.1:8000
rust/target/release/vimbai-local --profile full        # every address the gateway routes
rust/target/release/vimbai-local --port 9000 --data-dir ~/vimbai-data
```

There is nothing to install: the binary is the installation. `scripts/make_offline_bundle.sh` puts a release build in a
directory with a note saying what it is, for a machine that has no Rust toolchain.

## Profiles

1. *core* (default) - the bookkeeping loop an offline machine needs: identity (register / login), book-sync (Books and
   memberships), accounting (chart of accounts and the double-entry journal), departmental-accounting and
   audit-compliance. `/health` reports `services_loaded: 5` and nothing skipped.
2. *full* - the same process answers every address the gateway routes; the flag changes what `/health` says it is for,
   because there is no second process to start.

## How it works

1. *One binary, one process.* The engine's own router answers everything, so an offline machine cannot disagree with the
   cluster about what a sale did to the ledger: same code, same arithmetic, same posting rules.
2. *The store is a file.* `<data-dir>/store.json` is written after every mutating statement and swapped into place in one
   step, so a crash leaves the previous file rather than half of a new one. `VIMBAI_STORE_FILE` is set by the runtime, so
   an offline install cannot fall back to a Neo4j server it does not have.
3. *The gateway's rules, in the process.* In the cluster the Go gateway verifies the caller's token, injects `X-User-Id` and
   refuses a Book the caller is not a member of; nothing does that on a laptop, so the runtime does: `/identity` and
   `/health` are open, everything else needs a Bearer token (401), the verified id replaces any client-supplied header,
   and a Book the caller is not an active member of is refused (403) by asking the engine that holds it.
4. *Published addresses, from the gateway's own manifest.* Some services publish under a prefix the engine does not use, so
   `/book-sync/books` has to reach `/books`. The manifest is compiled into the binary, longest prefix first, matched as a
   whole segment; anything it does not say to strip is forwarded unchanged.
5. *The signing secret lives with the data.* `<data-dir>/jwt_secret` is created on first boot, readable only by its owner
   (0600 on unix), and reused after that, so a token in a person's hand still works after the process restarts.

## What is not here

Nothing is left on Python, so there is no service the on-device runtime leaves to the cluster.
