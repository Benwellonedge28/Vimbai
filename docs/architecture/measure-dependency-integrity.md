# Work package 25 — Keep Book measure dependencies valid

Source baseline `42a2d2f81c94b14394f86169429be9cef1339b89`. Proposed design.
## Observed behavior
`rust/crates/domains/accounting/src/measures.rs::create_measure` validates operands for composite definitions. `delete_measure` finds a row by name and deletes it without checking incoming dependencies. A still-configured composite can then refer to an operand that has disappeared.

## Contract
Build reverse references from the same typed operand parser used by the reader and creator. Include every operand position, including the minuend of a difference and both parts of a ratio. Compare the stored Book-local measure names using the existing name semantics. In `delete_measure`, read the target and incoming references and conditionally delete within one store transaction. Put the same concurrency guard around `create_measure` so a concurrently added reference cannot point to a deleted measure. Return direct dependent names in deterministic sorted order. Use the existing API error envelope through `rust/apps/api-server/src/accounting_book.rs`. Say “This measure is used by [names]. Remove or replace those definitions before deleting it.” Where the history cannot be read completely, refuse the write and explain the read failure. Avoid truncated scans and avoid cascading deletes.

In `apps/flutter/lib/features/progress/presentation/what_i_measure_page.dart`, preserve the existing list and display the refusal next to the delete action. `apps/flutter/lib/features/progress/data/measures_reading.dart` carries the server's outcome. The engine continues to compute with the Book's own definitions under ADR-017. This operation writes no accounting transactions and leaves existing definitions outside the target Book untouched.

## Existing rows
Keep IDs and definition names. A dangling operand already present must appear as an unavailable measure with the missing operand named. Preserve the underlying row for repair. After an authorised replacement or deletion, the rest of the Book's definitions must remain unchanged.

## Tests
- In `rust/apps/api-server/tests/measures_http.rs`, add `measure_delete_names_each_direct_dependent`, with both difference and ratio and a same-named measure in another Book.
- In the shared store tests, add `concurrent_measure_create_and_delete_leave_no_dangling_operand` on both providers.
- In the Flutter measure tests, add `dependent_measure_refusal_keeps_the_row_visible`.
- In the measures module, add `legacy_dangling_operand_is_unavailable_without_blocking_other_measures`.

Accept when the public list remains readable after a refused deletion and concurrent changes cannot create an invalid new definition.
