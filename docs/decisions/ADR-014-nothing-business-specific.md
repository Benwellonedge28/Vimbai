# ADR-014: Nothing business-specific is code - a Book's measures, profile and labels are its own data

**Status:** accepted, 2026-11-25
**Depends on:** ADR-003 (one crate per business domain), ADR-013 (a report is a view over any window)
**Amends:** nothing

## The problem

"Fit every business" fails in a predictable way. A system ships a dashboard for a shop, then a second dashboard for a
service business, then a flag for a farm, then a column for a contractor's retentions, and after a few years every
industry has a branch in the code that only that industry exercises. Nothing can be tested together, nothing can be
changed without fear, and a business that does two things gets whichever branch somebody guessed.

The other failure is the opposite one: a system with no defaults, where "configurable" means the owner must assemble
their own accounting before it says anything at all.

## The decision

1. **What varies is data set per Book.** Account numbers and names, currencies, tax names and rates, the fiscal
   calendar, the industry profile, which measures lead and their thresholds, dimensions (cost centre, project, branch,
   department, fund), labels and number formats. None of these appears in the engine as a constant.

2. **What does not vary is code, deliberately.** Debits equal credits, entries are immutable, a closed month refuses
   writes, amounts are exact at any magnitude, and one authority posts. A system that let a person configure these
   would not be accounting.

3. **A measure is a definition, not a code path.** A measure is composed from primitives the engine already holds -
   which accounts, by type or tag, which side, over which window, divided by what. Gross margin is two account types
   and a division; debtor days is a balance over a flow; margin per hectare is the same with a dimension. Adding a
   measure a business wants is a row of data, never a new branch.

4. **An industry is a profile that chooses from one vocabulary.** The engine keeps one fixed set of measures. A profile
   selects which of them lead, in what order, what counts as a warning, and what to compare against. A shop leads with
   stock turns and margin per line; a service business with debtor days and the share of time sold; a manufacturer with
   work in progress and yield; a contractor with contract margin and retentions held; a farm with the season and margin
   per hectare. Same numbers underneath, different front page, and a business that does two things gets both lenses
   rather than a merged guess.

5. **Defaults that work, and stay editable.** Every Book opens with a usable chart, profile and measures. The owner may
   change them, and the engine may say when the postings disagree with the profile - a consultancy whose stock account
   moves every week is worth mentioning - but it never silently changes a Book's policy.

6. **A gate enforces it.** A business word in the engine, outside the file that holds the defaults, fails the build.
   "Not hard coded" is then a test rather than an intention.

## What this forbids

- An industry name, tax name, currency code, fiscal month or account number in engine logic.
- A measure that exists only as a branch in a handler.
- A profile that changes what is computed rather than what is shown first.
- An empty Book that answers nothing because the owner has not configured it yet.

## How this is verified

Today: `crates/domains/accounting/src/progress.rs` names no account, currency, tax or industry. Revenue and costs come
from the Book's own account types, cash from the money accounts the Book keeps, what is owed from the documents it
holds, and tax from the tax ledger - so a Book that calls its accounts something else reads correctly, which its tests
exercise.

Not yet built, and named in `docs/ROADMAP.md`: the measure vocabulary as stored definitions, the profile that selects
from it, and the gate that fails on a business word in the engine.
