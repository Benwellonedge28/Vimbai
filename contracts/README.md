# Contracts

The agreed API shape, written by hand for one service at a time and checked against the Rust that
answers it.

This directory used to be empty on purpose. An OpenAPI document written for all 138 services would
be 1,238 paths nobody had read, and a contract nobody has read is worse than no contract: it looks
authoritative while being wrong. So the loop is proven on one service first.

## What exists

`costing.yaml` covers the overhead absorption service: ten operations, the money rules, the Book
scope. `job-costing.yaml` covers a different shape - path parameters, a query filter, a running total
that moves when a cost is charged, and an aggregate that sums several jobs. `treasury.yaml` covers cash
optimisation, the first contract from a second crate, where an operation replaces a stored set rather
than adding to it. `accounting.yaml` covers the cash flow statement: a ledger record, created and read
but never edited, whose totals the service derives rather than accepts. `equity.yaml` covers equity
changes, where a transaction may state its amount or leave it to be derived from shares and price,
and where the filter parameter is called `tx_type`. `assets.yaml` covers the fixed asset register, the
first contract from the assets crate and the first that describes records two services share. All six
were written from their Rust handlers, not from the route table.

The cash flow contract caught the wire being lossy. Reading how an amount reaches a client showed
`wire()` publishing a JSON number where `rust/MONEY.md` promised a string, so every figure was
rounded on the way out while the graph held it exactly - and all three earlier contracts had promised
the string, because each contract test compared the contract only to itself. `wire` now publishes
`to_fixed(6)`, and `test_the_wire_answers_the_exact_string_the_contracts_promise` reads that function
so the contracts and the wire have to agree.

The treasury contract settled where a rate sits. An interest rate is money, because the service
multiplies it into an amount, so it travels as an exact string; a margin is a ratio for reading, so it
travels as a number. "Looks like a rate" is not the test - whether arithmetic depends on the figure is.

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

## Clients are not generated here yet

There is no generated client in this repository at the moment. The Python one that lived in `sdk/python/`, and the
generator that wrote it, were Python, and they went with the rest of the Python. What the contracts buy without a client
is the last check in the list above: the contract is asserted against the Rust that answers it, so the document cannot
drift from the behaviour.

The clients the target architecture names (Rust, Dart, TypeScript) belong to the developer platform, generated into its
`sdks/` beside the sandbox that certifies them, so that a client and the sandbox cannot disagree about what the API
promises. Two promises from the deleted client are worth keeping when that happens: an amount is never a float, and a
call is never made without both `X-User-Id` and `X-Book-Id`.

## What comes next

1. Every routed service backed by the accounting, costing, equity and treasury crates now has a
   contract. The next five services to gain one will be the ones ported next, so the contract and the
   port ship together rather than the contract trailing the code.
2. Then the sibling services that share a shape with a contract already written, which is where
   starting from an existing contract begins to pay off.
3. Events in `contracts/events/`, once something publishes them.

A domain that has not been read into a contract has no contract. That is the rule that keeps this
directory honest, and it is why the remaining 120-odd services are not here yet.

## Layout

```
contracts/costing.yaml       overhead absorption: the worked example
contracts/events/            event schemas, once events are published
contracts/compatibility/     versioning and deprecation rules
```
