# Exact amounts: the `Money` contract

Vimbai must be able to hold **any** amount, at any magnitude, without losing a
unit. IEEE-754 `f64` cannot: it carries about 15 significant digits, so a
balance of 1,234,567,890,123,456,789.55 loses its cents before the first
arithmetic operation, and a currency in hyperinflation overflows to infinity
around 1.8e308.

`vimbai_common::Money` is the type to use for anything that is money.

## What it guarantees

| Property | `f64` | `Money` |
| --- | --- | --- |
| Significant digits | ~15 | unbounded |
| Largest value | ~1.8e308 | unbounded |
| `0.1` added a million times | 100000.00000000166 | exactly 100000.0 |
| Splitting 100.00 three ways | 33.333333... (loses a cent) | 33.34 + 33.33 + 33.33 |
| Round-trip through JSON | lossy | exact (string encoded) |

## Rules

1. **Amounts cross the wire as strings.** `Money` serialises as a JSON string
   and accepts a string or a number on the way in. Never declare a transport
   field as `float`/`number`: `123456789012345678901.55` sent as a JSON number
   is already wrong in the client's parser.
2. **Never do arithmetic in `f64` and convert afterwards.** Convert at the
   boundary, compute in `Money`, format at the end.
3. **Round once, at the edge.** `Money` keeps full precision through every
   step; call `.round(2)` or `.to_fixed(2)` only when producing a report
   figure. Rounding mid-calculation reintroduces the error the type removes.
4. **Split with `allocate`, not with division.** `total.allocate(&weights, 2)`
   distributes a total so the parts add back exactly, giving spare minor units
   to the largest remainders. Dividing and rounding each share independently
   does not.
5. **`to_f64_lossy()` is for charting only.** It is lossy by definition and is
   never a valid intermediate for a monetary total.

## Example

```rust
use vimbai_common::Money;

let balance = Money::parse("123456789012345678901234567890.55")?;
let rate    = Money::parse("0.075")?;

// 7.5% of a 30-digit balance, exact to the cent
let interest = balance.percentage(&rate, 2);
assert_eq!(interest.to_fixed(2), "9259259175925925917592592591.79");

// split a total across three cost centres without losing a unit
let weights = [Money::from_i64(1), Money::from_i64(1), Money::from_i64(1)];
let parts = interest.allocate(&weights, 2);
assert_eq!(Money::sum(&parts).to_fixed(2), interest.to_fixed(2));
```

## Where it is in use

| Module | Amount handling |
| --- | --- |
| `vimbai_common::money` | the type itself, and everything above |
| `vimbai_treasury_fx::cash` | investment optimisation, reconciliation, liquidity ratios |

The remaining calculation crates still compute in `f64` because they are
verified against Python services that already did so; converting them means
converting the Python side first, so that parity is proven on the new type
rather than assumed. The seam contract is: **Python `Decimal` / string in,
Rust `Money`, string out.**

## Why the seam matters

A Rust library that accepts exact decimals is not enough on its own. The
FastAPI services currently declare amounts as `float`, which truncates a large
figure before the calculation ever starts, and pydantic will happily accept
`1e400` as `inf`. Any service moving to `Money` must also change its models to
`Decimal` (or `str`) so the precision survives the HTTP layer.
