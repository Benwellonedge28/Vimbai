# Analytics and alerts

How a Book compares with the world it trades in, and how it finds out that something needs attention before a person goes looking.

Crates: `rust/crates/domains/analytics`, `rust/crates/platform/alerts`

## What it owns

1. Performance benchmarking: the metrics an industry is measured on, where one Book falls against them, and the peers it is being compared with. A benchmark whose peer set is unstated is an opinion, so the set is part of the answer.
2. Alerts: the rules that raise a notice, the notice itself, and whether it was read. An alert that nobody sees is worse than no alert, which is why raising one and reading one are both recorded.

## Services and addresses

| Service | Prefix |
| --- | --- |
| `alerts-service` | `/alerts` |
| `performance-benchmarking-service` | `/performance-benchmarking` |

## What it does not own

The arithmetic being benchmarked. This crate compares figures; it does not compute a profit margin or a ratio, and does not keep a second copy of a statement to read them from. Notices raised by a run rather than by an alert rule come through the notifications crate (`platform.md`).

## Tests

Benchmarking is unit-tested against a stated peer set, and the alert path is covered by tests that raise a notice, read the list back, and prove a read notice is no longer counted as unread.
