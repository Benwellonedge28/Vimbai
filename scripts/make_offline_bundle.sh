#!/usr/bin/env bash
# Build a fully offline install bundle for the Vimbai local runtime:
# a wheelhouse of every requirement, so the target machine needs no
# internet. Run ON a machine of the target OS/architecture, or pass
# --platform/--python-version/--only-binary for a cross-platform wheelhouse.
#
#   scripts/make_offline_bundle.sh out/
#   # cross-platform example (from Linux, targeting Windows 3.12):
#   scripts/make_offline_bundle.sh out/ --platform win_amd64 \
#       --python-version 3.12 --only-binary=:all:
#
# On the target machine:
#   pip install --no-index --find-links out/wheels -r vimbai-local/requirements-local.txt
#   python3 -m vimbai_local

set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "usage: $0 OUT_DIR [extra pip download args...]" >&2
  exit 1
fi

OUT="$1"
shift || true
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

mkdir -p "$OUT/wheels"
python3 -m pip download \
  -r "$REPO_ROOT/vimbai-local/requirements-local.txt" \
  -d "$OUT/wheels" "$@"

cp "$REPO_ROOT/vimbai-local/requirements-local.txt" "$OUT/"
echo ""
echo "Offline bundle ready in: $OUT"
echo "Target machine install:"
echo "  pip install --no-index --find-links $OUT/wheels -r $OUT/requirements-local.txt"
