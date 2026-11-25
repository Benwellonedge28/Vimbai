# Vimbai design work packages in dependency order

Repository baseline [728f9a2](https://github.com/Benwellonedge28/Vimbai/commit/728f9a22a5371f40fce7278c31566f8a074f2884). This review set follows [the roadmap](https://github.com/Benwellonedge28/Vimbai/blob/728f9a2/docs/ROADMAP.md) and [the design-document rules](https://github.com/Benwellonedge28/Vimbai/blob/728f9a2/AGENTS.md). Each linked design specifies one work package. Source references were checked again against review base [2384123](https://github.com/Benwellonedge28/Vimbai/commit/238412346b032133ca7a471060e1b3e9817c317e).

| Order | Document | Why here |
| --- | --- | --- |
| 1 | [Enforce Book policy](book-policy-gate.md) | First outstanding P12 enforcement item. |
| 2 | [Resolve posting accounts](posting-account-resolution.md) | Implements ADR-015 using the policy boundary from 01. |
| 3 | [Book access and navigation](book-access-and-navigation.md) | Resolves the membership transition before new privileged screens or actions are exposed. |
| 4 | [Complete financial reports](../domains/financial-report-completeness.md) | Protects bank reconciliation, ageing and the posting flow that consumes ageing. |
| 5 | [Portable Book recovery](portable-book-recovery.md) | Defines what survives device failure and how restored work syncs under Book permissions. |
| 6 | [Android pilot release](android-pilot-release.md) | Can ship test data early. Live pilot acceptance requires 04 and 05. |
| 7 | [Commercial subscriptions](commercial-subscriptions.md) | Uses the account boundary from 03 and distribution path from 06. |
| 8 | [Referrals and partner awards](referral-awards.md) | Qualifies awards from the payment and refund events defined in 07. |

## Decisions for review

- Confirm how existing Book memberships on organisation-owned Books should be carried forward. The code and ADR-010 describe different fallback behavior.
- ADR-013 and standard rules say a clean month closes automatically. The roadmap still shows this as open. Reconcile that roadmap row before reusing it as an implementation dependency. The roadmap also leaves the long-term treatment of manual posting controls open.
- The current “How am I doing” screen and measures editor already exist. Give the remaining decision-view work its own acceptance criteria after the repository’s roadmap defines the user action it should improve.
- Vimbai’s storage and data migration plan already has ADR-007, ADR-011 and store-move. Keep those decisions as the starting point for any additional provider or cutover design.
- This set replaces the broad PR #64 approach for these eight packages. A later source audit should scope separate packages for Windows distribution, international accounting and tax coverage, government, integrations, and other work that needs a concrete roadmap decision. Apply the independent requirements in the owning domain. Record any proposed policy change in its ADR.
