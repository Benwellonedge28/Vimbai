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

  # The deployment-store tests need a Postgres to run against. Rather than leave them to skip - a test that skips is a
  # test nobody runs - this starts a throwaway cluster when the server binaries are installed, and says so plainly when
  # they are not. Nothing is installed: a machine without Postgres still gets a clean preflight and an honest line.
  step "a Postgres for the deployment-store tests"
  pg_started=0
  pg_dir_used=""
  if [ -z "${VIMBAI_TEST_POSTGRES:-}" ]; then
    if command -v initdb >/dev/null 2>&1 && command -v pg_ctl >/dev/null 2>&1; then
      pg_dir=$(mktemp -d)
      # initdb refuses to run as root, and a container usually is: the package's own service account is used when it exists
      pg_user=$(id -un)
      pg_run=""
      if [ "$pg_user" = "root" ]; then
        if id postgres >/dev/null 2>&1; then
          pg_user="postgres"
          chown -R postgres "$pg_dir"
          pg_run="su postgres -c"
        fi
      fi
      pg_started_here=0
      if [ -n "$pg_run" ]; then
        if $pg_run "initdb -D '$pg_dir/data' -A trust -U '$pg_user'" >/dev/null 2>&1 \
          && $pg_run "pg_ctl -D '$pg_dir/data' -o '-p 55499 -k $pg_dir -c listen_addresses=127.0.0.1' -l '$pg_dir/log' start" >/dev/null 2>&1; then
          pg_started_here=1
        fi
      elif initdb -D "$pg_dir/data" -A trust -U "$pg_user" >/dev/null 2>&1 \
        && pg_ctl -D "$pg_dir/data" -o "-p 55499 -k $pg_dir -c listen_addresses=127.0.0.1" -l "$pg_dir/log" start >/dev/null 2>&1; then
        pg_started_here=1
      fi
      if [ "$pg_started_here" = 1 ]; then
        export VIMBAI_TEST_POSTGRES="host=127.0.0.1 port=55499 user=$pg_user dbname=postgres"
        pg_dir_used="$pg_dir"
        pg_run_used="$pg_run"
        pg_started=1
        echo "  started a throwaway Postgres on port 55499"
      else
        echo "  could not start a throwaway Postgres; the deployment-store tests will skip"
      fi
    else
      echo "  no Postgres installed; the deployment-store tests will skip"
    fi
  else
    echo "  using VIMBAI_TEST_POSTGRES"
  fi

  step "workspace tests"
  (cd rust && cargo test --workspace) || fail "cargo test --workspace"

  if [ "$pg_started" = 1 ]; then
    if [ -n "${pg_run_used:-}" ]; then
      $pg_run_used "pg_ctl -D '$pg_dir_used/data' -m immediate stop" >/dev/null 2>&1
    else
      pg_ctl -D "$pg_dir_used/data" -m immediate stop >/dev/null 2>&1
    fi
    rm -r -- "$pg_dir_used" >/dev/null 2>&1
  fi
fi

if [ "$status" = 0 ]; then
  printf '\npreflight clean\n'
else
  printf '\npreflight found problems above\n'
fi
exit "$status"
