# Open the right Book and show actions the caller can use

Baseline `728f9a22a5371f40fce7278c31566f8a074f2884`. Depends on ADR-010. Land before exposing newly gated actions from 02, 04 and 05.

## Existing behavior

`rust/crates/platform/tenancy/src/access.rs::allows` returns `Access` and checks active organisation membership before falling back to an active Book membership. Its `may_configure` and `may_write` methods are coarse role checks. `rust/apps/api-server/src/tenancy_gate.rs::check` checks membership, and `book_door` provides the membership answer. The fallback observed in code also executes for organisation-owned Books without active organisation membership. ADR-010 describes an organisation-owned Book as using its organisation membership, so resolve that discrepancy explicitly before changing how existing users reach Books.

## Permission model and navigation

The proposal is to preserve active standalone Book assignments and require an explicit organisation-authorized role for access to organisation Books. Inventory organisation-owned Books with users who depend on the fallback, report owners and affected membership edges, and require the owner to retain or remove each grant before switching the Book to the clarified rule. Record the decision and transition in ADR-010 in the same implementation PR. An organisation owner or admin may assign access. A member can inherit only the scope of their documented membership, narrowed by action policy.

| Operation | Proposed authorization |
| --- | --- |
| Read a Book or its reports | Active assignment or permitted organisation inheritance |
| Record ordinary facts | Read access and write capability |
| Change the chart, controls or measures | Explicit configuration ability |
| Reopen a closed period | Explicit period authority, actor and reason |
| Export a backup with pending work | Explicit export capability and available local keys |
| Manage membership | Owner or assigned administrator, with last-owner protection |

Keep `access::allows` the common membership authority. Add server decisions for named actions and attributes such as period state, role and Book. Resolve role and attributes on the server. Apply decisions to real handler entry points including period reopen, sync and backup. Expose a Book-scoped capability response through `tenancy_gate.rs::book_door` with a policy version. The gateway and local runtime use the same semantics. Denial at the client is a presentation choice. The server still refuses a direct request.

In `apps/flutter/lib/app/home_page.dart::_activateBook`, load capabilities before mounting Book actions. Extend `apps/flutter/lib/app/book_services_catalog.dart::BookServiceEntry` with a capability and section. The Book home shows entry points only: Record, Money, Customers and suppliers, Reports, and Book settings. Mount Cash Book, Bank Statement and reconciliation under Money. Mount ageing under Customers and suppliers. Keep existing route destinations and restore selected Book context only after revalidation; clear it on Book switch or revocation.

## Errors and compatibility

Show “You can read this Book, but you need permission to reopen this period.” During offline operation, use a versioned last-known capability state under the chosen offline policy and revalidate queued writes on reconnect. Old clients continue to receive server denials. Roll out the read-only capability response first, then the UI, then each enforcement switch after the membership audit.

## Acceptance tests

- `revoked_org_membership_follows_the_recorded_transition` covers inherited and retained explicit access.
- `a_book_reader_cannot_reopen_a_period_directly` crosses the gateway and engine entry points.
- `switching_books_drops_the_previous_books_capabilities` tests delayed Flutter responses and stale route state.
- Done when every listed action appears in an auditable permission table, access decisions agree across runtimes, and migrated Books preserve their owners.
