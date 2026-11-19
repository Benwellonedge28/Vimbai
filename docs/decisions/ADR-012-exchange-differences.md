# ADR-012: Books are kept in one money; differences are reported, never absorbed

**Status:** accepted, 2026-11-19
**Depends on:** ADR-007 (one system of record)
**Amends:** nothing

## The problem

Vimbai records foreign-currency documents already: a document names its money, states its rate, and the engine carries
the total, the discount and each tax into the Book's own money before a line posts. That is enough to record a sale. It
is not enough to answer the question a business actually asks, which is whether it made money.

A Harare business invoices 1,000 GBP at 36 ZWG, is paid at 38 ZWG, and holds 500 GBP across a month in which the rate
moves to 40. Three separate differences arise: 2,000 ZWG realised on settlement, 2,000 ZWG unrealised on the balance
held, and further differences again when the books are translated for a board that reads in USD. Record none of them and
the profit figure is wrong by more than the sale was worth. Record them in the wrong place and the profit figure is
right but the tax return is wrong.

Two designs were considered and rejected.

**Rejected: a second set of books in a second money, reconciled daily.** Many Zimbabwe businesses keep a ZiG ledger for
ZIMRA and a USD ledger for management. It reads as a feature and behaves as a fraud surface: two truths that will one day
disagree, and whoever has something to hide uses the convenient one. It also makes consolidation impossible, because
neither set of books is the record.

**Rejected: a presentation figure frozen onto every journal entry.** It looks efficient, since the translation is done
once at posting rather than repeatedly at report time. But translation is not a posting-time act: assets and liabilities
are translated at the closing rate of the period being reported, income and expenses at that period's average. A
presentation rate stored in an entry can never be the right rate for a later report, so the books become untranslatable.

## The decision

1. **One ledger, kept in the functional currency.** Every entry carries its functional amount, plus the transaction
   currency and the rate it came from, so it can always be re-read at the rate it was raised under. No presentation
   figure is stored on an entry.

2. **A rate is a dated, sourced row, never a value someone overwrites.** A rate states the pair, the figure, the day it
   applies from, and who says so (official, bank, market). A newer rate supersedes an older one and the older one stays.
   Nothing in the system calculates a rate for itself: the FX engine answers, and the ledger records what it was told.

3. **Which rate restates an item depends on what the item is.** Money held and money owed at the closing rate. Income
   and costs at the average. What the owners put in, and things bought rather than money (stock, machines, buildings), at
   the rate of the day they arose. A devaluation therefore never invents a profit on a machine or on stock.

4. **Three differences, three accounts, never netted.** Realised on settlement, unrealised on restatement at a period
   end, and cumulative on translation for consolidation. Each keeps its sign, so a loss is never folded into a gain.

5. **Translation leaves a reported leftover.** Whatever the closing, average and historical rates leave over is the
   cumulative translation adjustment, reported in equity. It is the figure that makes a translated balance sheet balance
   honestly instead of by a hidden rounding.

6. **The official rate posts; the market rate is shown beside it.** Posting at the official rate keeps the return
   matching what the accountant files. The market rate is recorded alongside, and a gap wide enough to change the story
   (five per cent or more) is reported plainly, because a business whose official books show a profit and whose market
   books show a loss must be told.

7. **One ledger, translated as often as needed.** ZIMRA and RBZ read the functional ledger translated to ZWG; the board
   reads it translated to USD. Same entries, two views, so the views cannot disagree.

8. **Hyperinflation is the owner's decision, recorded.** The software shows nominal and restated figures. Whether a
   period is hyperinflationary under IAS 29 is decided by a named person, and that decision is recorded with the period.
   Software does not declare a country hyperinflationary on its own.

## Where this lives

1. `rust/crates/common/src/currencies.rs`: what a code means, how exact its money is, and one conversion, in exact
   decimals, rounded to the money being converted into.
2. `rust/crates/common/src/fx.rs`: what an item is and therefore which rate restates it, the difference between what a
   balance was carried at and what it is worth, the translated set of books, the cumulative adjustment, and the official
   against market gap.
3. `rust/crates/domains/banking`: the dated, sourced rate rows themselves and the currency service that answers with them.

Both modules are in `vimbai_common`, so every crate that touches money inherits the same answers and no domain crate
carries a second opinion about a rate.

## What is not yet built

1. The dated rate table as stored rows with sources, and the lookups that read a rate as at a day.
2. Settlement posting: recognising the realised difference automatically when a foreign debt is paid.
3. The period-end revaluation run, and the lock that follows it.
4. Consolidation across entities with more than one functional currency, and per-currency reporting.
