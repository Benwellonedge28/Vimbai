# Work package 71 — Make bank quality scores traceable to individual observations

Code reviewed at [3a2c2ca1](https://github.com/Benwellonedge28/Vimbai/commit/3a2c2ca16978df88a95ad6b1c1decb2148a8707b).

## Why the observation identity matters

`add_quality_metric` in `rust/crates/domains/banking/src/relationship.rs` creates a stored observation but returns the metric name in the response's ID field. A zero passed through the score parser becomes one. `get_quality_metrics` filters the relationship's rows after fetching a Book-level collection and reports an empty average as zero. These behaviors make two visits with the same metric name difficult to distinguish and can make absent evidence look like a score.

## Observation contract

Use the existing `BANK_QUALITY_METRIC` record kind for a versioned observation. Require a scoped relationship ID, stable observation key, metric name, integer score one through five, effective date and authenticated recorder. Optional notes and source reference remain separate fields. Resolve the relationship before reading its metrics. Persist the unique observation key per Book and relationship, a canonical input fingerprint, and the actual stored record ID in one write. For duplicate matching input, return that ID. For changed input using the same key, return a conflict. Accept corrections through a new observation referencing the prior ID and recording the reason. The predecessor and successor must share Book, relationship and metric, with one successor per predecessor.

List observations by relationship and effective date with ID as a tie-breaker. A date-range summary uses the latest unreplaced observation in each correction chain whose original effective date lies in the effective window. Return count and exact numerator and denominator for the mean, with null average for no active observations. A saved score is an operational review, independent of ledger balances and bank authorization.

## Upgrade the route and register

Update `post_quality_metric` and `get_quality_metrics` in `rust/apps/api-server/src/records.rs`. The opt-in response returns the real ID in `observation_id` and the dimension in the existing name field. Keep the old ID meaning for unversioned requests during ADR-004 transition. Apply scope, relationship and date bounds before pagination. Read earlier observer rows by their stored IDs, preserving old ratings. Where an old score lacked the original submitted value, mark it legacy and skip guessed backfill. An authorized reviewer may supersede it with a documented score.

For invalid input say “Enter a whole-number score from 1 to 5.” For changed replay say “This observation key was already used. Open the original score to correct it.” An inaccessible relationship gives the same message as an unknown one.

## Verification

- In the relationship unit tests add `quality_score_requires_explicit_integer_without_zero_default`.
- In `rust/apps/api-server/tests/bank_relationship_http.rs` add `quality_observation_ids_distinguish_repeated_metric_names` and `quality_history_filters_before_paging_and_reports_empty_as_unknown`.
- In the shared provider target from [package 47](subsidiary-journal-posting-receipts.md), add `concurrent_quality_corrections_preserve_one_successor`.

Release when a reviewer can reopen an individual score and independently reconstruct the displayed summary from its observations.
