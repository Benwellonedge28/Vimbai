# Dependency rules

What each layer of the Rust workspace may depend on, and the gate that refuses anything else.

The rules are enforced, not described and hoped for. `cargo run -p xtask -- boundaries` applies them, and
`rust/xtask/tests/dependency_rules.rs` fails the build when one is broken. The rule identifiers below are the same
identifiers the code prints, and a test asserts the two lists agree, so this page cannot drift from the workspace.

## The families

| Family | Where it lives | What it is |
| --- | --- | --- |
| common | `rust/crates/common` | Shared vocabulary: money, identifiers, hashing, dates, normalisation |
| kernel | `rust/crates/kernel/*` | The ledger: `ledger-core` decides posting integrity, `ledger-services` reads and reconciles |
| calculations | `rust/crates/calculations/*` | Arithmetic with no state and no store: amounts in, amounts out |
| platform | `rust/crates/platform/*` | Identity, access control, events, documents, notifications, integrations, offline sync, cache, alerts, webhooks, admin |
| domain | `rust/crates/domains/*` | Business rules and orchestration for one domain of the business |
| adapters | `rust/crates/adapters/*` | The property store and the JSON wire seam |
| application | `rust/crates/application` | Use cases composing domains for a caller |
| apps | `rust/apps/*` | Binaries: the API server and the on-device runtime |
| tooling | `rust/xtask` | Repository gates and generators |

The family is read from the path, not from the crate name, and a crate living where no family claims it fails the
gate. A new crate therefore needs a home rather than a naming convention.

## The rules

| Rule | Family | May depend on | Why |
| --- | --- | --- | --- |
| R1 | common | nothing | It is the vocabulary everything shares; a dependency here is a cycle waiting to happen |
| R2 | kernel | common, kernel | The posting authority must not be able to borrow a business rule |
| R3 | calculations | common, kernel, calculations | Arithmetic stays pure. This rule is what stopped a calculation crate importing a domain to reach one function |
| R4 | platform | common, kernel, platform, adapters | Infrastructure may use the store; it may not grow a business rule |
| R5 | domain | common, kernel, calculations, platform, adapters | A domain may not import another domain. Two domains talk through a published surface |
| R6 | adapters | anything below it | The edge of the system: the store and the wire seam reach inward |
| R7 | application | everything except apps and tooling | Use cases compose the layers beneath them |
| R8 | tooling | nothing | The gates must build before the workspace does, so they cannot be made to agree with the code by the code |
| R9 | suite | anything in the workspace | An integrity suite calls the published surface from outside; nothing may depend on a suite, or the audit would become part of what it audits |

Everything not listed is forbidden. In particular no crate may depend on an `apps` crate, and no crate may depend on
`xtask`: a library that links a program is no longer a library.

## Tolerated dependencies

One dependency breaks a rule on purpose. It is recorded in the code as an exception with its reason, so it is visible
rather than accidental.

| Rule | From | To | Reason |
| --- | --- | --- | --- |
| R2 | vimbai_ledger_services | vimbai_store | The kernel's reading half reconciles control accounts and reports integrity by reading the ledger back out of the store. The posting authority, `vimbai-ledger-core`, stays pure and store-free, which is the guarantee that matters; reading is not posting |

Two properties keep a tolerance from becoming a habit. The gate fails if the dependency disappears, so a stale exception
cannot outlive its reason. And a test requires this page to name it, so it cannot be tolerated quietly.

## Posting integrity is one definition, not one rule

A dependency rule cannot see the copies that did the original damage, because those copies were local functions that
happened to answer the same question. So the gate also scans for a *definition* of the ledger authority outside the
kernel: `validate_lines`, `validate_money_lines`, `compute_entry_hash` and `check_reversal` may be called from anywhere
and defined only in `kernel/ledger-core`. Calling the kernel is the point; re-deriving its answer is the defect.

## What the gate does not cover

- Unsafe code. `#![forbid(unsafe_code)]` is checked by the crate gate in `rust/xtask/src/crates.rs`, which sweeps every
  crate root. It is not repeated here; a second copy of a check is the thing these rules exist to prevent.
- The store's internal purity, meaning that no calculation module lives inside the database adapter. Same gate as above.
- Duplicate HTTP registrations. That is `rust/xtask/tests/repository_rules.rs`, and the address audit behind it.
- Whether the dependency is a good idea. A rule can only refuse an edge; a design argument still has to happen.

## Running it

```
cargo run -p xtask -- boundaries             # violations, and exit 1 if there are any
cargo run -p xtask -- boundaries --graph     # every crate, its family and its dependencies
cargo run -p xtask -- boundaries --rules     # the rule table as this page states it
cargo run -p xtask -- boundaries --exceptions
```
