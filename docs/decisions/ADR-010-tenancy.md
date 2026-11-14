# ADR-010: An organisation is the tenant, and a Book belongs to one

**Status:** accepted, 2026-11-14
**Supersedes:** nothing
**Depends on:** ADR-002 (ledger-core is the posting authority), ADR-007 (one system of record)

## The problem

Before this decision the platform had members and Books and nothing in between. `book_id` was a property the store
stamped on a record, membership was a per-Book row, and the gateway and the engine each had their own idea of who might
read a Book. Nothing could express "these twelve Books are one client's", and nothing could answer "may this person open
this Book?" without a row for every person on every Book. On a real deployment that row does not exist: the accountant
is hired by the client, not invited to each Book one at a time.

There was also no tenant boundary at all. Every record lived in one pool, distinguished only by the Book it named, so
"show me everything belonging to this client" was a query nobody could write.

## The decision

1. **An organisation is a platform record.** It has a slug that is unique, a name, and the person who opened it, who is
   its first owner. Organisation records are not scoped to a Book, because they are what Books are grouped by.
2. **Membership is its own record**, naming the organisation, the person and their role. Adding a member and changing a
   member's role are separate acts from opening a Book.
3. **A Book belongs to exactly one organisation**, or to none. A Book with no organisation keeps the pre-tenancy
   behaviour: membership is per Book. There is no half-belonging and no Book in two organisations, because a
   consolidated report over an organisation must not double-count a Book that another organisation also claims.
4. **One access rule decides who may open a Book** - `tenancy::access::allows` - and it is the only copy. It answers from
   the organisation's membership when the Book has an organisation, and from the Book's own membership rows otherwise. A
   member of the organisation reaches the Book without a per-Book row; a stranger is refused.
5. **The rule is enforced by whoever holds the Book.** The engine's gate refuses with 403 before a handler runs, so a
   client that reaches the engine directly is refused exactly as one that arrives through the gateway. The gateway asks
   the same function rather than keeping a second copy, which is what it used to do.
6. **Ownership is protected at the last owner.** The last owner of an organisation cannot be demoted or removed, because
   an organisation with no owner cannot be administered and no other rule can repair it.
7. **Moving a Book in requires both rights**: administering the destination organisation and holding the Book. Moving a
   Book out of an organisation is not offered, because it would silently widen access to everybody the Book's own
   membership rows name rather than narrowing it.

## What this costs

A refusal is now possible where an empty list used to be returned: a non-member asking for a Book's records gets 403,
not `[]`. That is a deliberate change to the wire. A client that relied on an empty answer to mean "nothing here" now
learns the difference between "nothing" and "not yours", which the old behaviour concealed.

Organisation records are platform records and are read through the platform door, so a handler that lists them must not
also filter by Book. The store gained a find-platform-records-by-property door for the member listing, because a member
list is not scoped to any one Book.

## What this does not decide

It does not decide roles beyond the three that exist (owner, admin, member), and it does not decide billing, because no
part of the platform bills anybody. It does not make an organisation a security boundary at the storage layer: the
records of two organisations in one deployment share tables, and isolation is enforced by the access rule and the Book
scoping that already existed. A deployment that needs physical separation runs a second deployment, which ADR-007's
system-of-record decision already permits.

The engine's own doors - the calculator catalogue and the health route - are outside tenancy entirely: they take no Book
and hold no records.

## How this is held

Fifteen tests: ten in `rust/crates/platform/tenancy` covering the owner, unique slugs, adding and demoting members, the
last-owner protection, role validation, member listings, an organisation's Books and moving a Book in; five over HTTP
covering an organisation member reaching its Book with no Book membership row, a stranger refused by the engine itself,
and the organisation addresses answering. `docs/domains/organisations.md` records what the domain owns, and
`docs/architecture/system-boundaries.md` records where the rule sits.
