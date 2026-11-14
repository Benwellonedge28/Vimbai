# Audit

An auditor's own records: the engagements they run, the findings those engagements raise, the sample they drew to find them, and the report the findings add up to.

Crate: `rust/crates/domains/audit`

## What it owns

1. Engagements: the scope, the period, the client, the findings raised during the work and the state each finding is in.
2. Sampling: a sample drawn from a population by attribute or by value, stratified or not, with the draw recorded so the auditor can show how the sample was reached rather than only what it contained.
3. The immutable trail, resource versions, lineage and chain integrity: what changed, in what order, and whether the record of it still holds together. Compliance assessments and the reports drawn from them are answered here too.

Sampling is the part worth stating plainly: a sample that cannot be reproduced is not evidence. The draw is deterministic given its parameters, which is what makes an auditor's working papers survive a question about them.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `audit-compliance-service` | `/audit-compliance` |
| `audit-engagement-service` | `/audit-engagement` |
| `audit-sampling-service` | `/audit-sampling` |

## What it does not own

The audit opinion. This crate records the work and the findings; the report is a document a person signs, and nothing here will sign it for them. Nor does it own the ledger trail itself - the hash chain is `ledger-core`'s, and this crate reads it.

## Tests

The engagement suite proves a finding can be raised, read back and reported; the sampling suite proves a sample over a population returns the drawn members and refuses a population it cannot draw from; the compliance suite walks the immutable trail and the chain integrity answer, including the case where a posted line has been altered and only a re-stamped chain reports intact.
