# Organisations and tenancy

An organisation is a tenant. It owns Books, a Book belongs to exactly one organisation, and a person reaches a Book by
belonging to the organisation that owns it. This page records what exists at P2, why the model is shaped the way it is,
and what the rule is that every component asks.

## Why the model is shaped this way

Before P2 the middle of that sentence was missing. An organisation was a record with a name; membership was a single
`organization_id` field on the account; a Book was stamped with whoever opened it. "May this person read this Book" was
therefore answerable in one place only - the Go gateway - and inside the engine there was no answer at all, so a second
door to the same records (the on-device runtime) had to have its own rule, and did.

Three decisions follow from that:

1. **An organisation is a platform record, not a Book record.** It has to be found before a Book is known, and it is the
   same organisation in every Book it owns. Scoping it per Book would mean one organisation per Book.
2. **Membership is a record of its own.** The same person can belong to several organisations with a different role in
   each, and a field on the account has nowhere to say what happens when somebody is removed. The membership record is
   keyed by organisation and person, so adding somebody twice is an update rather than a second row.
3. **A Book belongs to at most one organisation.** A Book with an organisation is governed by that organisation's
   members; a Book with none keeps the membership it was opened with, which is every Book that existed before P2.

## The rule

One function answers it, `vimbai_tenancy::access::allows(graph, user_id, book_id)`:

```
Book missing                                  -> 404, "not there" (not a refusal)
Book has an organisation:
    caller is an active member of it          -> allowed, as that role, via Organisation
Book has no organisation, or the above failed:
    caller is an active member of the Book    -> allowed, as that role, via Book
otherwise                                     -> 403
```

Two details matter and are pinned by tests:

- A **non-member is not told the organisation exists**. Reading somebody else's tenant answers 404, the same answer an
  unauthenticated stranger gets, because a 403 on a named organisation is a directory of other people's tenants.
- The Book's own membership is still honoured when the Book has an organisation. A Book keeps the memberships it was
  opened with; the organisation adds members, it does not silently evict the people already working in it.

## Where it is enforced

| Component | How it applies the rule |
| --- | --- |
| `gateway/go` | Before proxying, as it always did; now asked of the engine rather than read from a local table |
| `rust/apps/api-server` | `tenancy_gate::guard` in front of the record routes: a request naming a Book is refused unless the caller may open it |
| `rust/apps/local-runtime` | Serves the same `api::router()`, so it inherits the same gate; its in-process `book-sync` check calls the same function |
| `GET /books/{id}/membership` | Answers through `access::membership_check`, so the gateway and the engine cannot disagree |

The gate is deliberately narrow: it asks only when a request names a Book (`x-book-id`), takes the caller from the
injected `x-user-id`, and leaves a missing Book to the route's own 404. `/health`, `/books`, `/identity` and `/ws` answer
before a Book is known, so they pass through.

## Addresses

| Address | What it does |
| --- | --- |
| `POST /identity/organizations` | Open an organisation; the opener becomes its owner |
| `GET /identity/organizations` | The organisations the caller belongs to, each with their role |
| `GET /identity/organizations/{id}` | One organisation, for a member |
| `GET /identity/organizations/{id}/members` | The people in it, with their roles |
| `POST /identity/organizations/{id}/members` | Add somebody, or change their role (owner or admin only) |
| `GET /identity/organizations/{id}/books` | The Books the organisation owns |
| `PUT /identity/organizations/{id}/books/{book_id}` | Move a Book in; needs administering the organisation *and* holding the Book |
| `GET /books/{id}/membership` | The gateway's question, answered by the same rule |

## Roles

`owner`, `admin`, `member`. An owner or an admin may add members and move Books; a member may read and write inside the
organisation's Books. An organisation keeps at least one owner: demoting the last one is refused with a conflict rather
than leaving nobody able to administer it.

A role outside that list is refused by name when it arrives, rather than stored and silently ignored.

## What P2 does not do

- **Renaming the record.** The label stays `Organization`, the spelling the shipped identity surface already wrote, so no
  existing record or client is orphaned.
- **A per-organisation role catalogue.** Roles are the three the platform recognises. Per-tenant custom roles are a later
  decision, and would belong beside these rather than instead of them.
- **Deleting an organisation, or removing a member.** Not yet decided: what happens to the Books, and who may remove whom.
  The last-owner rule already refuses the case that would leave a tenant unadministrable.
- **Data residency or billing.** An organisation is a boundary for membership; it is not yet a billing or residency unit.

## Tests

`rust/crates/platform/tenancy/src/organisation.rs` (six: opening with no prior context, shared names getting separate
slugs, one person in two organisations with different roles, a member refused the administrator's actions, the last owner
protected, an unknown role refused) and `.../access.rs` (four: a Book no organisation owns, a Book one does, a missing
Book, and the gateway's answer naming the route). Over HTTP,
`rust/apps/api-server/tests/organisations_http.rs` (five: opening and listing, a member reading the organisation's Book
while a stranger is refused, moving a Book needing both halves, the last owner, and the member listing).
