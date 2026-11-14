# Compliance and security

The rules a Book must be seen to follow, and the record that shows it did: regulatory checks, SOX controls, privacy administration, risk assessment, and the data-protection posture that says who may see what.

Crate: `rust/crates/domains/compliance`, `rust/crates/domains/security`

## What it owns

1. Regulatory compliance: the obligations that apply, the checks against them, and the results kept as a history rather than a current status, because a check that passed last quarter is a fact about last quarter.
2. SOX compliance: the controls, their dashboard, and the evidence that a control operated.
3. Risk assessment: a risk scored in, filtered by category and by level, reassessed with a mitigation and a new status, closed, and refused when it names a category the register does not have. The dashboard answers per company and refuses an empty read with a message a person can act on rather than a bare zero.
4. Privacy administration: the dashboard of what personal data is held and why.
5. Security and zero-trust data: the classification of a record and the rule that says a lower clearance cannot read a higher classification.

Compliance checks seed themselves on first read: that six-check default is the reason the list answers before anything has been configured, and it is pinned by a test so it cannot quietly become an empty page.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `privacy-admin-dashboard-service` | `/privacy-admin-dashboard` |
| `regulatory-compliance-service` | `/regulatory-compliance` |
| `risk-assessment-service` | `/risk-assessment` |
| `sox-compliance-service` | `/sox-compliance` |
| `zero-trust-data-service` | `/zero-trust-data` |

## What it does not own

The audit opinion and the immutable trail, which live in `audit.md`. Nor does it authenticate anybody: identity and access live in `platform.md`, and this crate asks them who a caller is.

## Tests

The risk suite covers score, category filter, level filter, reassessment with a mitigation, the dashboard, an empty dashboard, close, a ninth-category refusal and the prefixed call. The compliance-checks list is pinned by a test that proves six checks appear on first read and that a one-half rate is recorded, and the empty-company case still answers.
