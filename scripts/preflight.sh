#!/usr/bin/env bash
# Everything CI will check, run before you push.
#
# Usage: ./scripts/preflight.sh          (everything)
#        ./scripts/preflight.sh rust     (workspace only, fast)
#        ./scripts/preflight.sh python   (the Python that is still Python)

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
# rustup installs outside the default PATH in some shells
export PATH="$HOME/.cargo/bin:$PATH"
status=0
step() { printf '\n== %s\n' "$1"; }
fail() { printf 'FAILED: %s\n' "$1"; status=1; }

want_rust=1; want_python=1
if [ $# -gt 0 ]; then
  want_rust=0; want_python=0
  for arg in "$@"; do
    case "$arg" in
      rust) want_rust=1 ;;
      python) want_python=1 ;;
      *) echo "unknown target: $arg"; exit 2 ;;
    esac
  done
fi

if [ "$want_rust" = 1 ]; then
  step "rustfmt"
  (cd rust && cargo fmt --all -- --check) || fail "cargo fmt"
  step "clippy"
  (cd rust && cargo clippy --workspace --all-targets -- -D warnings) || fail "clippy"
  step "workspace tests"
  (cd rust && cargo test --workspace) || fail "cargo test --workspace"

  # The tests below drive the on-device runtime as a real process, because that binary is what a machine installs. A
  # debug build is enough and is already warm from the tests above.
  (cd rust && cargo build -p vimbai_local) || fail "cargo build -p vimbai_local"
  export VIMBAI_LOCAL_BIN="$PWD/rust/target/debug/vimbai-local"
fi

if [ "$want_python" = 1 ]; then
  step "python formatting"
  # CI lints each service directory, so check the same directories rather than the repo root
  services=$(find . -maxdepth 3 -name main.py -not -path "./node_modules/*" -printf '%h\n' | sort -u)
  for dir in $services; do
    python3 -m isort --check-only --quiet "$dir" || fail "isort $dir"
    python3 -m black --check --quiet "$dir" || fail "black $dir"
  done
  step "python tests"
  python3 -m pytest tests -q || fail "pytest"
fi

if [ "$status" = 0 ]; then
  printf '\npreflight clean\n'
else
  printf '\npreflight found problems above\n'
fi
exit "$status"
