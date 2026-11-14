# Accounting

The ledger and everything that writes to it: the chart of accounts, the journal, the trial balance, the statements those entries add up to, and the services that keep one company's books inside a Book shared with others.

Crate: `rust/crates/domains/accounting`

## What it owns

Double-entry bookkeeping is the whole of it. An entry is posted with its lines, balanced, and from then on immutable; a correction is a reversing entry, never an edit. Everything else in this page reads those entries or records the facts they are made of:

1. the chart of accounts and the ledger behind it, and each account's postings walked in from the account rather than recomputed;
2. the statements: trial balance, balance sheet, trading account, profit and loss, cash flow and the cash flow statement's schedule;
3. the cashbook, petty cash, the sales and purchases ledger controls, and the control account reconciliation that proves the subledgers agree with the ledger;
4. revenue recognition, intercompany and departmental accounting, fund accounting, and IFRS reporting;
5. the standards catalogue, the configuration that switches a standard on, and the compliance check that records the result.

A departmental create answers with the department and the cost centre it opened, in one statement, because a department nobody can charge to is not a department. A balance is derived by replaying the entries rather than stored: a stored balance is a second source of truth that drifts from the entries it must reconcile to.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `accounting-service` | `/accounting` |
| `accounting-standards-service` | `/accounting-standards` |
| `balance-sheet-service` | `/balance-sheet` |
| `cash-flow-service` | `/cash-flow` |
| `cash-flow-statement-service` | `/cash-flow-statement` |
| `cashbook-service` | `/cashbook` |
| `company-accounting-service` | `/company-accounting` |
| `departmental-accounting-service` | `/departmental-accounting` |
| `fund-accounting-service` | `/fund-accounting` |
| `ifrs-reporting-service` | `/ifrs-reporting` |
| `intercompany-service` | `/intercompany` |
| `revenue-recognition-service` | `/revenue-recognition` |
| `trading-account-service` | `/trading-account` |

## What it does not own

Posting integrity. Balance enforcement, immutability and the hash chain belong to the ledger kernel (`ledger-core`), and this crate calls it rather than keeping a second copy of the rule (ADR-002). Amounts are `common::Money`, exact and unbounded, not floats. Costing arithmetic lives in `rust/crates/domains/costing`.

## Tests

Every module carries its own tests over the arithmetic and the records, and the HTTP suite in the api-server's tests walks the published addresses so a client's call is proven, not assumed. The journal-line migration is pinned by tests that read lines back joined to their entry, and the balance refusal is exercised with a one-unit difference in the thirtieth decimal place.
