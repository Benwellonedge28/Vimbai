# ADR-014: Nothing business-specific is code - a Book's measures and labels are its own data

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
   calendar, which measures lead and in what order, dimensions (cost centre, project, branch, department, fund), labels
   and number formats. None of these appears in the engine as a constant.

2. **What does not vary is code, deliberately.** Debits equal credits, entries are immutable, a closed month refuses
   writes, amounts are exact at any magnitude, and one authority posts. A system that let a person configure these
   would not be accounting.

3. **A measure is a definition, not a code path.** A measure is composed from primitives the engine already holds -
   which accounts, by type or tag, which side, over which window, divided by what. Gross margin is two account types
   and a division; debtor days is a balance over a flow; margin per hectare is the same with a dimension. Adding a
   measure a business wants is a row of data, never a new branch.

4. **Defaults that work, and stay editable.** Every Book opens with a usable chart and a usable set of measures. The
   owner may relabel them, change what they count, add their own or delete one, but the engine never silently changes a
   Book's policy.

5. **A gate enforces it.** A business word in the engine, outside the file that holds the defaults, fails the build.
   "Not hard coded" is then a test rather than an intention.

## What this forbids

- An industry name, tax name, currency code, fiscal month or account number in engine logic.
- A measure that exists only as a branch in a handler.
- An empty Book that answers nothing because the owner has not configured it yet.

## Never to be built

Two things that were once on the roadmap are struck out by the owner's ruling of 2026-11-25, and are removed from every
design document rather than deferred:

1. **Industry profiles.** No profile picks a trade's leading measures, order or thresholds, and no industry list ships.
   The measures a Book reads are the rows it holds (ADR-017), in the order the owner puts them. A shop and a
   consultancy get the same nine figures to start from and each owner shapes them; the engine never guesses a trade.
2. **Threshold warnings and watchpoint nudges.** No measure carries a warning line, and Vimbai does not send a message
   because a figure crossed one. A figure is reported as it stands; what is worth noticing is the owner's judgement,
   not a constant in the engine.

The reason is the one this record already gives: a threshold is a business-specific opinion dressed as a default, and
one that arrives unasked is a branch in the code by another name. A Book that must be told what "low" means is not
autonomous, and a Book that decides it for them is guessing.

## How this is verified

Today: `crates/domains/accounting/src/progress.rs` names no account, currency, tax or industry. Revenue and costs come
from the Book's own account types, cash from the money accounts the Book keeps, what is owed from the documents it
holds, and tax from the tax ledger - so a Book that calls its accounts something else reads correctly, which its tests
exercise.

Built since: the measure vocabulary as stored rows (ADR-017), which a Book owns and may change. Still to come: the gate
that fails on a business word in the engine.

## The gate that holds it

`rust/xtask/src/business_words.rs` reads the engine and reports every line that writes a four-digit account number
outside a test. A number in the engine is the clearest way to break this decision, because it does not fail: it posts a
business's money to whatever account happens to carry that number, or to nothing at all, and the entry still balances.
Four digits in quotes is how every account number in this repository is written and how none of its dates, years,
percentages or amounts are, so the rule needs no list of what an account is called.

The chart a Book opens with is the one place allowed to write a number, because that is data written as code. Everything
else must ask the Book for its accounts.

What the gate found is a backlog, not a permission: twenty-eight files still write one hundred and eighty-two numbers
between them, chiefly the equity, partnership and assets journals, which post to `"3300"` and `"1000"` as though every
business numbered its chart that way. The foreign-exchange settlement was the first to go: it posted to `"1010"`,
`"1100"`, `"4800"` and `"5800"` whenever the caller named nothing, and it now asks the Book for its bank, its receivable or
payable and its exchange gain and loss by purpose, so a business that numbers its chart differently settles correctly. The list in `STILL_NUMBERED` is a ratchet, and the test fails when
a file is added to it, when a file's count rises, or when a file's count falls without its figure being lowered. So the
backlog can only shrink, and every fix has to be recorded in the same commit that lands it.
