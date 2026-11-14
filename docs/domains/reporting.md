# Reporting

Getting the right figure to the right person without anybody assembling it by hand: the report, the dashboard over it, the schedule that produces it, and the distribution that delivers it.

Crate: `rust/crates/domains/reporting`

## What it owns

1. Reports: defined, produced from recorded figures, and read back. A report is a query with a name and a period, not a spreadsheet somebody maintains.
2. The dashboard: the figures a Book watches, assembled once rather than recomputed by each client.
3. Report automation and distribution: which report runs when, and who receives it. It answers on its own prefix because the flat automation addresses belong to the automation domain, and only one service may hold a given address.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `dashboard-service` | `/dashboard` |
| `report-automation-service` | `/report-automation` |
| `report-distribution-service` | `/report-distribution` |
| `reporting-service` | `/reporting` |

## What it does not own

The arithmetic in a report. This crate assembles figures the domain crates computed; a report that recomputes a balance is a report that can disagree with the ledger it describes. Nor does it own the statements themselves - those are produced by `accounting.md` and merely read here.

## Tests

The reporting suite covers a report defined and produced over a period and read back, and the dashboard's assembly. Distribution is tested by proving the recipient list and the report identifier are both recorded rather than the send being assumed.
