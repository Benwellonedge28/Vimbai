#!/usr/bin/env bash
# Put everything a machine needs for offline Vimbai into one directory.
#
# It used to build a wheelhouse: the local runtime was a Python package with a requirements file. It is a Rust binary
# now, so the offline install is the binary and a note saying how to run it - nothing to resolve, nothing to install.
#
#   scripts/make_offline_bundle.sh out/
#
# Then copy out/ to the machine and run vimbai-local.

set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "usage: $0 OUT_DIR" >&2
  exit 1
fi

OUT="$1"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

mkdir -p "$OUT"
(cd "$REPO_ROOT/rust" && cargo build --release -p vimbai_local)
cp "$REPO_ROOT/rust/target/release/vimbai-local" "$OUT/"

cat > "$OUT/README.txt" <<'NOTE'
Vimbai local runtime
====================

One binary. No runtime to install, no packages to resolve, no database to run.

    ./vimbai-local                                  # http://127.0.0.1:8000, core bookkeeping profile
    ./vimbai-local --port 9000 --data-dir ~/vimbai-data
    ./vimbai-local --help

Your book is kept in <data-dir>/store.json, written after every change and replaced in one step, so a crash leaves the
previous file rather than half of a new one. The directory also holds jwt_secret, made on first boot, which is what
signs people in: back up the directory and both come with it.

Off by default and never needed: the internet. An offline machine serves the same handlers as the cluster.
NOTE

echo ""
echo "Offline bundle ready in: $OUT"
echo "Copy it to the machine and run:  ./vimbai-local"
