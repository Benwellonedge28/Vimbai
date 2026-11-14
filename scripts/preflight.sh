#!/usr/bin/env bash
# Everything CI will check, run before you push.
#
# The reason this exists: native/ is deliberately outside the Rust workspace because it
# links libpython, so `cargo test --workspace` never builds the crate that Python actually
# loads. A change can be perfectly green locally and break the wheel, which is what happened
# when duplicate material variance functions were removed from the wrong side.
#
# Usage: ./scripts/preflight.sh          (everything)
#        ./scripts/preflight.sh rust     (workspace only, fast)
#        ./scripts/preflight.sh seam     (the Python extension only)

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
# rustup installs outside the default PATH in some shells
export PATH="$HOME/.cargo/bin:$PATH"
status=0
step() { printf '\n== %s\n' "$1"; }
fail() { printf 'FAILED: %s\n' "$1"; status=1; }

want_rust=1; want_seam=1; want_python=1
if [ $# -gt 0 ]; then
  want_rust=0; want_seam=0; want_python=0
  for arg in "$@"; do
    case "$arg" in
      rust) want_rust=1 ;;
      seam) want_seam=1 ;;
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

  # The on-device runtime serves the retired bookkeeping services from the engine, so the engine binary is part of
  # what the Python tests below exercise. A debug build is enough and is already warm from the tests above.
  (cd rust && cargo build -p vimbai_api) || fail "cargo build -p vimbai_api"
  export VIMBAI_ENGINE_BIN="$PWD/rust/target/debug/vimbai-api"
fi

if [ "$want_seam" = 1 ]; then
  step "the Python extension (outside the workspace, so checked separately)"
  (cd rust/native && cargo check) || fail "cargo check in native/"
  # The parity tests import the compiled extension, and a stale build in site-packages will
  # happily answer with last week's arithmetic. Rebuild it so the seam tests mean something.
  if command -v maturin >/dev/null 2>&1; then
    step "rebuilding the extension so the seam tests are not testing an old build"
    # maturin develop insists on a virtualenv; build a wheel and install it instead, which
    # works the same way in a bare interpreter as CI does.
    (cd rust/native && maturin build --quiet --out /tmp/preflight_wheels \
      && python3 -m pip install --quiet --force-reinstall --no-deps /tmp/preflight_wheels/*.whl) \
      || fail "rebuilding and installing the extension"
  else
    echo "maturin is not installed: skipping the rebuild, the seam tests may use a stale build"
  fi
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
