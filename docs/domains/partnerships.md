# Partnerships

The form of business where the owners are the balance sheet: the agreement between them, the changes to who holds what, the revaluation that happens when one joins or leaves, and the dissolution that ends it.

Crate: `rust/crates/domains/partnerships`

## What it owns

1. The partnership agreement: who the partners are, their profit-sharing ratio, and the terms that only matter when something goes wrong.
2. Changes to the partnership: a new partner admitted, one retiring, and the goodwill arithmetic that a change of ratio forces.
3. Revaluation on a change: the assets marked to their current value, the gain shared in the old ratio and the new carrying amounts established in the new one. Getting the two ratios the wrong way round is the classic error, so the order is stated in the code and pinned by tests.
4. Dissolution: the order of payment on winding up, and the final distribution.
5. Family and community groups, which are partnerships with a different name and a different set of expectations, and which answer on their own prefix.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `family-community-group-service` | `/family-community-group` |
| `partnership-agreement-service` | `/partnership-agreement` |
| `partnership-changes-service` | `/partnership-changes` |
| `partnership-dissolution-service` | `/partnership-dissolution` |
| `partnership-revaluation-service` | `/partnership-revaluation` |

## What it does not own

The partners' drawings, which are ordinary ledger entries, or the tax treatment of a distribution. This crate decides the shares; `accounting.md` records the entries and `taxation.md` the tax.

## Tests

Each instrument is tested against a worked example, including the revaluation shared in the old ratio before the new one takes effect and the dissolution order. The published addresses are walked over HTTP so a client's call is proven.
