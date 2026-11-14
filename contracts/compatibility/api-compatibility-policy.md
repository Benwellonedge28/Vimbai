# API compatibility policy

What a client may rely on, and what changes are allowed to cost it.

## Versioning

The wire is versioned in the path: `contracts/openapi/v1/`. A change that breaks a client is `v2`, published beside
`v1` and served beside it until `v1`'s retirement date has passed. A change that does not break a client stays in `v1`,
which is why this document exists: "additive" is a claim that needs a definition rather than a feeling.

## Compatible in `v1`

- **A new address.** Adding `/npo/grants/{id}/revisions` breaks nobody.
- **A new method on an existing address**, unless the address was fetch-only and the new method changes its meaning.
- **A new field in a response.** A client that ignores unknown fields - which every JSON client does - is unaffected.
  A client that rejects unknown fields is already broken and this policy will not save it.
- **A new optional request field**, with a documented default that preserves today's behaviour.
- **A new value in an enum a client is expected to tolerate**, and only where the contract said so. `status` on a grant
  is one; `POST` versus `PUT` on a dividend payment is not.
- **A new error code inside the existing envelope.** The envelope is `{"error": "<string>"}`; a client that branches on
  the string is branching on prose, and the policy does not protect prose. A client that branches on the HTTP status is
  protected, so a status change is a breaking change and adding a code inside the same status is not.

## Breaking, and therefore `v2`

- Removing or renaming an address, a method, a field or a field's type.
- Tightening validation: a request that was accepted being refused is a break, even though the new answer is "more
  correct". This one is easy to do by accident, because a bug fix reads like an improvement to whoever writes it.
- Requiring a header, a parameter or a field that was optional.
- Changing what an address means while keeping its shape - the same path answering a different calculation. This is the
  worst break of the set because nothing fails until a figure is wrong.
- Changing the scope header's meaning, or accepting a request for one Book and answering for another.

## Money is never a float

Amounts travel as JSON strings and are `common::Money` end to end. A response that renders an amount as a JSON number is
a defect, not a compatibility question: a client parsing it would lose digits at the top of the range, and the ledger it
disagrees with is the one that is right. This is why `1.0` -> `1` is not an allowed "cleanup" anywhere in `v1`.

## Records and their identity

An address parameterised by `{id}` takes the record's id. Where a service addresses a record by something else - an
invoice by its number - the contract says so in the parameter's description, because a parameter named `{id}` that is not
an id is a trap a client cannot see. Changing which key addresses a record is breaking.

## How these are enforced

`cargo run -p xtask -- openapi --check` fails when a published address has no contract, when a contract names an address
nothing answers, when one address is documented twice, or when the committed specifications are not what the generator
writes. The schemas in `contracts/openapi/v1/services/` are the hand-written part and carry the detail a generator cannot:
they are edited by hand, and the wire shapes they describe are pinned by the per-domain HTTP tests.
