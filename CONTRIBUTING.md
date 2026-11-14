# Contributing to Vimbai

## Getting started

1. Fork the repository.
2. Branch from `main`: `git checkout -b feature/your-feature-name`.
3. Make the change.
4. Run `./scripts/preflight.sh`. It runs what CI runs: `cargo fmt --check`, `clippy -D warnings`, the whole workspace,
   the on-device runtime end to end, then the Go gateway suite and the Flutter app tests.
5. Open a pull request against `main`.

Read `docs/ROADMAP.md` first. Work follows that sequence, and a change that belongs to a later phase is usually better
landed when its phase arrives.

## Code style

### Rust

- `#![forbid(unsafe_code)]` in every library and application crate. No exceptions, no `#[allow]` escape hatch.
- `cargo fmt` and `clippy -D warnings`. Both run in preflight, so a red push is your own.
- Financial arithmetic uses `Money` from `crates/common`. A float in a financial path is a bug, not a shortcut.
- One crate per business domain. Arithmetic with no state goes in `calculations/`, business rules in `domains/`.
- Nothing posts to the ledger except through `crates/kernel/ledger-core`. See ADR-002.
- Money crosses JSON as a string, binary and encrypted blobs as base64.

### Go (gateway)

- `gofmt`, standard project layout, table-driven tests.
- The gateway routes, authenticates, rate limits and proxies. If a change adds arithmetic that belongs to a financial
  rule, it belongs in Rust instead.

### Dart (Flutter)

- `dart format`, `flutter analyze` clean, tests beside the feature.
- One UI. The React application is retired; do not add to it.

## Pull request checklist

- [ ] `./scripts/preflight.sh` is clean.
- [ ] A test fails without the change and passes with it.
- [ ] No duplicate implementation of an existing rule, calculator or route.
- [ ] No secrets, tokens or credentials in the diff.
- [ ] `CHANGELOG.md` updated under Unreleased when behaviour changes.
- [ ] An ADR added or amended if the change touches a recorded boundary.
- [ ] Any published address still answers at the same path with the same figures.

## Branch strategy

- `main` - the only long-lived branch, and what deploys.
- `feature/*`, `fix/*` - short-lived, squash-merged.

## Decisions

Boundary decisions live in `docs/decisions/` as ADRs. If a change contradicts an ADR, amend the ADR in the same pull
request, with the reason. A contradiction nobody recorded is how this repository ended up with three service registries
and two payment gateways.
