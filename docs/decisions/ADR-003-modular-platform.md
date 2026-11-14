# ADR-003: Modular platform, one crate per business domain

*Status:* accepted, 2026-11-14.

## Context

The proposed target architecture describes a workspace of `common`, `money`, `identifiers`, `errors`, `contracts`,
`kernel/`, `platform/`, `domains/`, `calculations/`, `application/` and `adapters/`. That is the right shape at the
family level. Below the family level, some of the proposal would create duplicates rather than boundaries, because
`crates/common` already owns money, identifiers and hashing, and the published contracts already live in `contracts/`.

## Decision

1. *One crate per business domain.* Accounting, finance, banking, taxation, budgeting, costing, treasury, assets,
   reporting, audit, compliance, equity, payroll, partnerships, invoicing, receivables, payables, workflow, automation,
   analytics, multimodal, npo, data, security, and payments (POS till support only, see ADR-005). A domain is not split
   into several crates because it has several features, and two domains do not share a crate because they are related.
2. *Crate families are honoured:* `kernel/` (ledger authority), `platform/` (identity, access control, events, documents,
   notifications, integrations, offline sync, cache, alerts, webhooks, admin), `calculations/` (pure arithmetic, no
   persistence), `domains/` (business rules and orchestration), `application/` (use cases), `adapters/` (database, wire).
3. *A calculation is not a service.* Arithmetic with no state lives in `calculations/` and is called directly. It does not
   get an HTTP address of its own unless a client already depends on that address.
4. *One crate is not one deployment.* The platform is a modular monolith with the gateway in front. Independent
   deployment requires a reason: isolation, scaling or availability. There is no per-service Deployment in the cluster,
   and the manifest gate fails if one reappears.
5. *The catalogue is the inventory.* `catalog/services/catalog.yaml` is generated from the gateway's route table and
   ownership, and pinned by a test. A second hand-maintained service registry is a defect.

## Recorded divergences from the proposed tree

| Proposed | Actual | Reason |
| --- | --- | --- |
| `crates/money`, `crates/identifiers`, `crates/errors` | `crates/common` | `common` already owns `Money`, ids, hashing, dates and the ledger primitives. Splitting would duplicate `Money`. |
| `crates/contracts` | `contracts/` at the root | Contracts are a published artefact consumed by tests and clients, not a library the backend links. |
| `calculations/cost-accounting` and other hyphenated names | `calculations/cost_accounting`, `capital_valuation`, `equity_payroll`, `group_accounting`, `market_risk`, `performance_measures`, `ratios_statements`, `tax_reporting`, `treasury_fx` | Directory names do not change crate identity. Renaming 16 paths is churn with no behaviour. See ADR-008. |
| `crates/domains/integrations` | `crates/platform/integrations` | Third-party plumbing, not a business domain with its own rules. |
| `crates/domains/payments` for Vimbai Pay | `crates/domains/payments` holds POS till support only | Vimbai Pay is not being built. See ADR-005. |

## Consequences

- The tree will not match the proposal file for file, and a reader comparing the two will find the table above.
- New capability lands as a crate in an existing family, or as a module inside a domain crate when it has no independent
  reason to exist.
- A domain may grow large. When it does, the answer is modules with clear boundaries, then a split only if the split
  removes a real coupling.
