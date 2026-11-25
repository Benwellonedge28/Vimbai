# Vimbai design continuation

Source review pinned to [2df3a76](https://github.com/Benwellonedge28/Vimbai/commit/2df3a7685b1109756fad0cca4d7b648b25790bdb). Read [packages 01–08](design-reading-order.md) first. The initial set was submitted in [PR #75](https://github.com/Benwellonedge28/Vimbai/pull/75). Each file is a separate proposed work package. The order below is the reading order. Read each file’s dependencies before scheduling implementation.

| Order | Work package | Implementation gate |
| --- | --- | --- |
| 09 | [Windows offline release](windows-offline-release.md) | The release path must be reproducible. Live offline use depends on proven portable recovery and Book authorization. |
| 10 | [Accounting policy and evidence](../domains/accounting-policy-evidence.md) | Establish explicit reporting coverage and evidence per version, then choose one profile for review. |
| 11 | [Jurisdiction tax rule versions](../domains/jurisdiction-tax-rule-versions.md) | Review the jurisdiction and its fixture with a qualified local reviewer before release. |
| 12 | [Statement import and replay](../domains/statement-import-and-replay.md) | Land complete statement-period reconciliation and the Book authorization contracts first. |
| 13 | [Government budget control pilot](../domains/government-budget-control-pilot.md) | Confirm one department’s authority, classifications and reporting rules, and name the reviewers before adoption. |
| 14 | [Partner event delivery](partner-event-delivery.md) | Begin only after the owner records a qualifying integration need under ADR-006. |

The government design covers appropriations and commitments for a first department. Country-specific reporting, procurement and treasury work require their own policy and evidence. The tax and accounting designs specify how to add, test and version coverage. A later profile must define the actual obligations and authoritative source for the jurisdiction selected.
