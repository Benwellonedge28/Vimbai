# Contracts

Where the agreed, versioned API shape will live. It is deliberately empty of specs: writing an
OpenAPI document by hand for 138 services would produce 1,238 paths of fiction that nobody validates.

## What exists today

Each Python service declares its own routes, and `catalog/services/*.yaml` records them as they are,
with the port, gateway path and auth requirement. That is a description of the current estate, not a
contract: it says what answers today, not what is promised to a client.

## What comes next

1. Pick one domain that already runs on Rust - `costing` or `treasury` are the furthest along.
2. Write `contracts/openapi/v1/<domain>.yaml` by hand, for the endpoints that domain will keep.
3. Add a test that compares the contract against the Rust routes, so the document cannot drift from
   the handler it describes.
4. Extend to the next domain only after that loop is proven.

A contract that is not tested against an implementation is a second opinion about the API, and this
repository has already learned what a second opinion costs.

## Layout

```
contracts/openapi/v1/<domain>.yaml   agreed HTTP shape, per domain
contracts/events/                    event schemas, once events are published
contracts/compatibility/             versioning and deprecation rules
```

Nothing in this directory is generated yet. When it is, the generator will read the catalogue for
identity and the handlers for shape, and a test will fail on any disagreement between the three.
