#!/usr/bin/env bash
# Everything CI will check, run before you push.
#
# Usage: ./scripts/preflight.sh          (everything)
#        ./scripts/preflight.sh rust     (workspace only, fast)

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
# rustup installs outside the default PATH in some shells
export PATH="$HOME/.cargo/bin:$PATH"
status=0
step() { printf '\n== %s\n' "$1"; }
fail() { printf 'FAILED: %s\n' "$1"; status=1; }

want_rust=1
for arg in "$@"; do
  case "$arg" in
    rust) want_rust=1 ;;
    *) echo "unknown target: $arg"; exit 2 ;;
  esac
done

if [ "$want_rust" = 1 ]; then
  step "rustfmt"
  (cd rust && cargo fmt --all -- --check) || fail "cargo fmt"
  step "clippy"
  (cd rust && cargo clippy --workspace --all-targets -- -D warnings) || fail "clippy"
  # The workspace tests include the binaries' own, which start the real process: build them first, or those tests have
  # nothing to start and skip instead of testing anything.
  step "binary tests need their binaries"
  (cd rust && cargo build -p vimbai_local -p vimbai_sync) || fail "cargo build -p vimbai_local -p vimbai_sync"
  export VIMBAI_LOCAL_BIN="$PWD/rust/target/debug/vimbai-local"
  export VIMBAI_SYNC_BIN="$PWD/rust/target/debug/vimbai-sync"

  step "workspace tests"
  (cd rust && cargo test --workspace) || fail "cargo test --workspace"
fi

if [ "$status" = 0 ]; then
  printf '\npreflight clean\n'
else
  printf '\npreflight found problems above\n'
fi
exit "$status"
