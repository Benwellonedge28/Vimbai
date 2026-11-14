# ADR-005: Vimbai Pay is not built

*Status:* accepted, 2026-11-14. Revisit only on the trigger in this record.

## Context

The proposed architecture gives Vimbai Pay a dedicated domain crate with payment lifecycle, collections, refunds,
reconciliation, merchant accounts, QR payments, payment links, recurring payments, a provider registry and settlement.

The repository does not have that. `crates/domains/payments` holds POS till support: terminals registered, takings
recorded, a queue drained in batches, a marking of what has been synced. That is bookkeeping for money taken elsewhere,
not a payment product.

A record-only rule used to keep a Python payment gateway alive: it recorded an authorised intent and never moved money.
That service was deleted, and the rule went with it, because the engine now records the same intent as ordinary data.

## Decision

1. Vimbai Pay is *not* built as part of this roadmap. It is not deferred behind a phase, it is out of sequence entirely.
2. No provider adapters, no settlement engine, no stored value, no wallet, no payment link product.
3. POS till support stays as it is. It is not renamed to Vimbai Pay and does not grow into it.
4. Money movement, if it ever happens, comes from an authorised external provider through a contract that is reviewed
   against the applicable licensing and settlement arrangements. Building a payment product is a regulatory decision
   before it is an engineering one.

## Trigger to revisit

All of these, not any one of them:

1. A specific provider relationship or licence is in place, in writing.
2. Tenancy exists (roadmap P2) and contracts exist (roadmap P6), because a payment domain keys on both.
3. There is a customer commitment that cannot be met by an integration with an external provider.

## Consequences

- The Flutter payments feature is POS screens, not a payment product. Documentation says so, so nobody builds against a
  capability that is not there.
- The platform records payments it did not take. That is a legitimate accounting function and it is already correct.
