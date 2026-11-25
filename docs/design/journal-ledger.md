# Journals and ledgers
Status: proposed clarification of ADR-002, ADR-009 and ADR-015.

A journal records chronological economic events; a ledger groups posted lines by account. They are views of the same posting facts, not two independent authoritative stores. Subledgers track detailed obligations or assets and reconcile to configured control accounts.

## Lifecycle
Draft → validated → approved where required → posted → reversed by separate mirror entry. Posting is atomic with sequence, hash chain, lines, account links and idempotency. Never edit posted entries. Reclassification uses transfer entry. Account resolution uses one shared policy-aware function; provisional accounts only under ADR-015 and never for control accounts.

## Integration
rust/crates/kernel/ledger-core owns balance, immutability, reversal and chain. ledger-services owns reconciliation/read models. accounting owns business validation and periods. Store adapter persists transaction; Flutter journals and ledger screens query authoritative projections. No second balance algorithm.

## Tests
Unbalanced entry rejected at exact precision; concurrent writers preserve sequence; repeated command returns same outcome; reversal mirrors original and cannot duplicate; journal totals equal ledger totals; subledger controls reconcile; closed-period and unauthorized writes fail. Crash recovery leaves either entire posting or none.
