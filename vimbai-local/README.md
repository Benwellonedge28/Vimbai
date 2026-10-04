# Vimbai Local — on-device, offline runtime

Run the entire Vimbai platform in **one Python process** on any machine —
Windows, macOS or Linux — with **no internet, no Docker, no Kubernetes, no
Neo4j server**. Data lives in a local folder and survives restarts.

## Quick start

```bash
pip install -r vimbai-local/requirements-local.txt

python3 -m vimbai_local                      # core bookkeeping profile, http://127.0.0.1:8000
python3 -m vimbai_local --profile full       # every registered service (318)
python3 -m vimbai_local --port 9000 --data-dir ~/vimbai-data
```

Open http://127.0.0.1:8000/docs for the combined interactive API.
On Windows, `py -m vimbai_local` from the repo root works the same.

## Profiles

1. *core* (default) — the essential bookkeeping loop: identity (register /
   login), book-sync (Books + memberships), accounting (chart of accounts +
   double-entry journal), departmental-accounting, audit-compliance.
   Boots in ~2s, tens of MB of memory — fine for a small laptop or
   single-board computer.
2. *full* — every service registered in `api-gateway/config/services.json`
   plus identity. Boots in ~25s cold, ~210MB memory. Services that fail to
   import are skipped with a warning instead of sinking the boot; the
   `/health` endpoint lists them.

## How it works

1. *One process:* every service's FastAPI app is imported and mounted at its
   gateway path, exactly like the bracket containers do it.
2. *Gateway contract in-process:* `vimbai_local/gateway.py` reproduces the
   production gateway — `/identity` is open (register/login), everything else
   needs a Bearer token; the verified user id is injected as `X-User-ID`
   (never trusted from the client); `X-Book-ID` is checked against active
   Book memberships (403 otherwise).
3. *Persistence without Neo4j:* each durable service ships a
   `fake_neo4j.py` driver implementing exactly the Cypher subset that service
   uses (the same code its test suite runs against). In local mode the
   session is wrapped so every mutation is snapshotted to
   `<data-dir>/<service>.json` and restored on boot.
4. *Sqlite services* (book-sync, personal-finance, npo-scale) point their
   databases into the data dir.
5. *JWT secret:* generated once, stored in `<data-dir>/jwt_secret` (0600 on
   POSIX). Delete it to invalidate all local sessions.

## The data dir

`--data-dir` (default `./vimbai-data`) holds everything: per-service store
snapshots, `book_sync.db`, JWT secret. Copy the folder to move your Books to
another machine. Delete it for a factory reset. Offline by design: nothing
in local mode makes a network call.

## Fully offline install (no pip access at run site)

`scripts/make_offline_bundle.sh` builds a wheelhouse for the current
platform:

```bash
scripts/make_offline_bundle.sh out/          # wheels for THIS machine's OS/arch
# on the target machine, no internet needed:
pip install --no-index --find-links out/wheels -r vimbai-local/requirements-local.txt
```

Copy the repo (or `git clone` it once) plus the wheelhouse to the target
machine and run. To prepare a wheelhouse for a *different* OS/arch, see the
`--platform` hints in the script.

## Notes and limits

1. Same auth and Book-isolation rules as production — verified by
   `tests/test_vimbai_local.py` and the end-to-end smoke
   (`scripts/smoke_e2e_flow.py`), which both pass against the local runtime.
2. Local mode is single-user-at-a-time friendly: it has no rate limiting or
   TLS; keep it bound to 127.0.0.1 (the default) unless you know what you
   are doing.
3. Cross-service calls that production makes over HTTP between containers
   are resolved in-process; where a sibling is not in the running profile
   the calling service degrades gracefully (same as production).
