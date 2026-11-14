# ADR-009: The kernel's reading half reads the store

*Status:* accepted, 2026-11-14.

## Context

The dependency gate has exactly one tolerated dependency: `vimbai_ledger_services` (the kernel's services) depends on
`vimbai_store` (the database adapter). The rule it bends says a domain or kernel crate must not know how records are
persisted.

The dependency is not incidental. `control_account` reconciles the control account by reading the ledger back out of the
store, `integrity` verifies a company's records by re-reading and re-hashing them, and both write their findings as
records. That is the kernel's reading half: work whose whole subject is the stored state.

The guarantee that matters is a different one, and it is not weakened: `vimbai-ledger-core` - the posting authority -
holds no store, no database driver and no I/O. It decides balance, immutability, reversal and the hash chain from values
it is handed, and the financial-integrity suite proves those properties from outside it.

## Decision

1. The dependency stays, and it is recorded here rather than as a comment in a gate file.
2. It is scoped to reading and reporting. A *posting* rule may not be moved into `ledger_services` to gain store access;
   posting stays in `ledger-core`, which cannot see a store.
3. The exception is pinned in the gate with this ADR named in its reason, and the gate test fails if an exception cites an
   ADR that does not exist. An untraceable tolerance is the thing being refused, not the tolerance itself.

## Consequences

- The gate reports one tolerated dependency, and every reader of that report can follow the reference and find the reason.
- Reversing it means introducing a read port (a trait the kernel defines and the adapter implements). That is a real
  change with a real benefit, and it belongs with the data-layer decision in P8 rather than in a close-out: done now it
  would be a trait with one implementation and one caller, which is a layer pretend to have.
- If the store is ever split by ownership (P8), this is the dependency that shows where the seam goes.
