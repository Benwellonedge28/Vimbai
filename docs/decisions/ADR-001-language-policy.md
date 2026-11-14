# ADR-001: Language policy

*Status:* accepted, 2026-11-14. *Supersedes:* nothing. *Superseded by:* nothing.

## Context

The platform began as a Python service estate: one directory per business capability, each its own process behind a Go
gateway. Correctness drifted because the same calculation existed in more than one service, and financial arithmetic ran
on floats in places. The migration replaced 141 service directories with a single Rust engine that answers every address
the gateway publishes, and deleted the Python rather than keeping it as a fallback.

## Decision

1. *Backend logic is Safe Rust.* Every library and application crate carries `#![forbid(unsafe_code)]`. The compiler
   enforces it; it is not a grep in CI. `cargo run -p xtask -- crates` asserts the attribute is present, and the check
   reads the crate root so a crate cannot carry it in a comment only.
2. *The gateway is Go, and only a gateway.* Routing, authentication, authorisation, rate limiting, resilience and
   observability. It carries no accounting arithmetic, no ledger posting and no financial settlement. A calculation that
   needs to happen belongs behind the gateway in Rust.
3. *The client is Flutter.* One Dart codebase for mobile, desktop and web. The React application is retired as a UI path.
4. *No Python anywhere in the repository.* Not as a service, not as a test harness, not as a build script, not as a
   configuration file for a tool that no longer runs. A capability that cannot be expressed in Safe Rust, Go or Dart is
   deleted rather than ported, as the PyO3 bridge was.
5. *Money is exact.* `Money` is a `BigDecimal` in `crates/common`, JSON carries it as a string, and no float enters a
   financial comparison. Amounts have no upper bound.

## Consequences

- A new capability has one obvious home: a domain crate, not a service directory.
- Dependency review matters. `forbid(unsafe_code)` binds this codebase, not its dependencies, so a dependency that needs
  `unsafe` internally is a decision to record, not one to make silently.
- Contributors who write Python have no path in. That is the intent: the estate the policy replaced lost correctness to
  exactly that kind of proliferation.
- The Go gateway can be replaced or rewritten without touching a financial rule, provided the route table and the
  authentication behaviour survive.
