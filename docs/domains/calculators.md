# Calculators

The catalogue behind every stateless calculation the engine answers, and the two doors that serve it: the list of what it holds, and the one that runs a named calculator over a payload.

Crates: `rust/crates/calculations/advisory`, `rust/crates/calculations/assurance`, `rust/crates/calculations/budgeting`, `rust/crates/calculations/capital_valuation`, `rust/crates/calculations/cost_accounting`, `rust/crates/calculations/equity_payroll`, `rust/crates/calculations/group_accounting`, `rust/crates/calculations/ifrs`, `rust/crates/calculations/market_risk`, `rust/crates/calculations/performance_measures`, `rust/crates/calculations/ratios_statements`, `rust/crates/calculations/reporting`, `rust/crates/calculations/tax`, `rust/crates/calculations/tax_reporting`, `rust/crates/calculations/treasury_fx`, `rust/crates/calculations/variance`

## What it owns

1. The catalogue: one map of name to calculator, built once in the engine, so a calculator is registered in exactly one place and the list a client reads is the list the runner can run.
2. Running one by name, with the payload the caller sent, answering the calculator's own figures rather than a wrapping shape nobody asked for.
3. The published addresses that a retired service used to answer with the same arithmetic, so a client that was never repointed still reaches the calculator rather than a second copy of the maths.
4. The exact-money rules every calculator in the catalogue follows: amounts are decimals of any size and precision, never floats, and a figure that cannot be computed exactly is refused rather than rounded.

## Deliberately not here

The domain crates own the arithmetic. A calculator in this catalogue calls the domain crate that knows the rule; the catalogue itself decides only which name reaches which function. Money movement, posting and settlement remain ledger-core's, and no calculator here posts an entry.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `calculation-api` | `/calculators` |
| `calculation-api` | `/calc` |
