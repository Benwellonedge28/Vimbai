# Contracts

The agreed API shape, written by hand for one service at a time and checked against the Rust that
answers it.

This directory used to be empty on purpose. An OpenAPI document written for all 138 services would
be 1,238 paths nobody had read, and a contract nobody has read is worse than no contract: it looks
authoritative while being wrong. So the loop is proven on one service first.

## What exists

`costing.yaml` covers the overhead absorption service: ten operations, the money rules, the Book
scope. `job-costing.yaml` covers a different shape - path parameters, a query filter, a running total
that moves when a cost is charged, and an aggregate that sums several jobs. Both were written from
their Rust handlers, not from the route table.

The second one settled a rule the first did not have to. A margin is a ratio, not money: `gross_margin`
and `avg_margin` are JSON numbers because they are for reading and ranking, while every balance -
`contract_value`, `total_cost`, `gross_profit`, `amount` - is an exact decimal string. The money check
allows the first and refuses the second.

`tests/test_contracts.py` holds it to the handlers in both directions:

* every path in the contract is a route the API serves, and its method matches;
* every route of that service is in the contract, so adding a route means writing it down;
* every money field is a decimal string, never a JSON number, and every contract states the rule the
  same way, so a client generated from one cannot refuse an amount another accepts;
* the keys a response promises are the keys the handler writes.

That last check is the one that matters. The first three say the contract points at something real;
the last says the answer it describes is the answer that comes back.

## The SDK is generated from the contract

`scripts/generate_sdk.py` reads every contract and writes `sdk/python/<domain>_client.py`, so a
client cannot promise a method the contract does not describe or miss one it does.
`tests/test_sdk.py` fails if the committed client stops matching the generator, and holds two
promises the money rule depends on: the client refuses a float amount before it reaches the wire,
and it refuses to be built without both `X-User-Id` and `X-Book-Id`.

```python
from sdk.python.costing_client import VimbaiOverheadAbsorptionClient

client = VimbaiOverheadAbsorptionClient(
    "https://api.vimbai.example", user_id="u-1", book_id="book-1"
)
rate = client.calculate_absorption_rate(
    cost_centre_name="Assembly",
    cost_centre_id="cc-1",
    budgeted_overhead="125000.00",
    budgeted_base_units="4000",
)
```

## What comes next

1. A third contract from a different domain: `treasury` (cash optimisation) or `equity`.
2. Then the absorption and job costing siblings that share their shapes, which is where generating
   from an existing contract starts to pay off.
3. Events in `contracts/events/`, once something publishes them.

A domain that has not been read into a contract has no contract. That is the rule that keeps this
directory honest, and it is why the remaining 120-odd services are not here yet.

## Layout

```
contracts/costing.yaml       overhead absorption: the worked example
contracts/events/            event schemas, once events are published
contracts/compatibility/     versioning and deprecation rules
sdk/python/                  generated from the contracts above
```
