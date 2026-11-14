# ADR-002: ledger-core is the sole posting authority

*Status:* accepted, 2026-11-14.

## Context

`crates/kernel/ledger-core` implements the three guarantees the platform depends on: every entry balances, every entry is
hash-stamped over its predecessor so the chain is tamper-evident, and a reversing entry is the exact mirror of the entry
it reverses with at most one reversal per entry. `crates/kernel/ledger-services` holds the control-account reconciliation,
integrity reporting and financial state machine that operate on that kernel.

Historically, several Python services each carried their own balance rule with a float tolerance, and a
`/validate-journal-entry` endpoint kept a second copy of the check. That duplication is what let the numbers drift.

## Decision

1. `ledger-core` decides whether a set of journal lines balances. No other crate re-implements that comparison.
2. `ledger-core` alone defines immutability and reversal. A posted entry is never edited; a correction is a reversing
   entry, and the reversal itself must pass the mirror check.
3. `ledger-core` alone owns the hash chain. A path that writes a posted entry without extending the chain is a defect.
4. The accounting domain orchestrates: it opens periods, records entries, validates *business* rules (account exists,
   period open, approval present), then hands the lines to the kernel. Orchestration is not authority.
5. `ledger-services` may read the ledger and report on it. It may not post and may not validate balance independently.

## Consequences

- A domain that needs a ledger writes through the kernel. If it cannot, the kernel is missing a concept and gets it,
  rather than the domain growing a private ledger.
- Testing the guarantees does not require reading every consumer. The financial-integrity suite (roadmap P3) proves the
  invariants once, against the kernel.
- The kernel is the smallest, most-reviewed surface in the repository. That is deliberate: it is the one place where a
  mistake is unrecoverable in the records rather than merely in a response.
