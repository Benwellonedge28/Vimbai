# Financial integrity: what is guaranteed, and by which test

This is the compliance record for the ledger. Every guarantee below is a claim about behaviour, and every claim names the
test that fails if the claim stops being true. Nothing here is asserted because the code is written carefully; it is
asserted because something checks it.

The suite is a crate of its own, `rust/tests/financial-integrity`, deliberately outside the kernel and the domains it
audits. A rule proven in the same file that implements it is a rule that can be edited into agreement with a bug.

```
cargo test -p vimbai_financial_integrity
```

## The guarantees

### 1. An entry balances, or it does not exist

Debits must equal credits exactly, and every line must move money one way only. A line that moves nothing is not a line;
a line that is both a debit and a credit is refused. The rule is decided by `ledger-core`, and the accounting domain
refuses the entry before writing anything.

Tests: `tests/invariants.rs::an_entry_balances_or_is_refused` (both doors, and that nothing was written),
`tests/properties.rs::random_entries_balance_once_double_entry_is_applied` (300 generated entry sets),
`tests/properties.rs::a_mirrored_entry_balances_with_no_plug_at_all`.

### 2. A posted entry is immutable

A posted entry cannot be edited and cannot be deleted. A draft can be discarded. A correction is a reversing entry, never
an amendment, because an amended entry would break the stamp the chain vouches for and every stamp after it.

Tests: `tests/invariants.rs::a_posted_entry_cannot_be_edited_or_deleted` (the edit is refused, the delete is refused, the
entry is unchanged, and a draft still goes).

The kernel itself has no notion of "posted": status is a domain concept, so the rule is proven where it is enforced and
the kernel proves the part it owns (the stamp changes if the content changes).

### 3. A reversal must be the mirror, and it nets to zero

A reversing entry must name the same accounts and swap the sides with the same amounts. A different amount is not a
reversal. An account the original never touched is not a reversal. A half-flipped set is not a reversal. When it is a
real mirror, every account the pair touched ends at exactly zero.

Tests: `tests/invariants.rs::a_reversal_that_is_not_the_mirror_is_refused`,
`tests/invariants.rs::a_reversal_nets_the_original_to_zero` (three accounts, net checked as `Money`, not as a float sum).

### 4. The ledger is tamper-evident

Each entry is stamped over its own payload and over its predecessor's stamp. Altering one posted line fails that entry's
stamp; re-stamping the altered entry fails the next entry, because that one chained over the old stamp. Rebuilding the
chain end to end verifies again: the guarantee is detection, not prevention, and this is the honest form of it.

Test: `tests/invariants.rs::the_hash_chain_breaks_when_a_posted_line_is_altered` (intact chain verifies, one altered line
is caught by name, a re-stamped entry breaks its successor, a fully rebuilt chain verifies).

### 5. No float enters the comparison path a posted entry uses

Amounts are exact decimals (`vimbai_common::Money`), unbounded in magnitude and precision. A difference of one unit in the
thirtieth decimal place is refused. The classic `0.1 + 0.2 != 0.3` trap does not exist on this path.

Tests: `tests/invariants.rs::an_exact_comparison_refuses_a_difference_a_float_would_swallow`,
`tests/properties.rs::the_float_door_has_a_boundary_and_the_exact_door_does_not`.

Two doors exist and they are not interchangeable. `validate_lines` is the float door, kept for a wire that still carries
JSON numbers; it rounds at ten decimal places so that `0.1 + 0.2` reads as `0.3`. `validate_money_lines` is the exact door
a posted entry uses. The safe direction of the difference is asserted, not assumed: the float door may refuse a set the
exact door balances (it is conservative), and it must never accept a set the exact door refuses.

Test: `tests/properties.rs::the_float_door_never_accepts_what_the_exact_door_refuses` (300 generated sets, both doors).

### 6. No unsafe code in the ledger kernel

Every kernel crate carries `#![forbid(unsafe_code)]`, and the attribute is the enforcement rather than a sentence in a
comment. The test reads the attribute out of each kernel `lib.rs` and also refuses an `unsafe` block in the source.

Test: `tests/invariants.rs::every_kernel_crate_forbids_unsafe_code`.

### 7. The port reproduced the retired Python services' figures

Every retired service whose figures were worth keeping has a fixture under `rust/tests/fixtures/parity/`, captured while
the service was still on disk: the request, the addresses it published, and the answer it gave. The engine must still
answer the same numbers at the same addresses. A fixture is never edited to match new output; a changed figure is a
decision and gets recorded as one.

Tests: `tests/parity.rs::the_treasury_analytics_service_figures_come_back_unchanged` (100 days of cash adequacy, 15 per
cent debt service), `tests/parity.rs::the_absorption_costing_service_figures_come_back_unchanged` (prime cost 2300,
production cost 3000, unit cost 30; 84000 absorbed over 28000 hours at 3.00 an hour, -12000 under-absorbed on short
hours; 250 plus a twenty per cent markup sells at 300), and
`tests/parity.rs::every_fixture_names_a_service_this_file_checks` (a fixture nothing asserts is a claim nobody reads).

## What is deliberately not claimed

- **Prevention.** The chain detects an altered line; it cannot stop somebody with write access from rebuilding the chain.
  Prevention is access control, which is a different mechanism with different tests.
- **A float-free wire.** Clients still send JSON numbers on some routes. The float door accepts them for figures a double
  can hold exactly (up to 2^53 units) and is conservative beyond that; the exact door is what posts.
- **Reversal uniqueness in the kernel.** The kernel decides whether a candidate entry is the mirror of an original; which
  entry may be reversed, and how many times, is decided where entries are held.
- **Balance of stored data.** The suite proves the rules the write path applies. A store edited outside the write path is
  caught by the chain, not by the balance rule.

## Where the rules live

| Rule | Decided by | Proven by |
| --- | --- | --- |
| Balance | `rust/crates/kernel/ledger-core/src/double_entry.rs` | `tests/invariants.rs::an_entry_balances_or_is_refused` |
| Immutability of a posted entry | `rust/crates/domains/accounting/src/ledger_book.rs` | `tests/invariants.rs::a_posted_entry_cannot_be_edited_or_deleted` |
| Reversal is the mirror | `rust/crates/kernel/ledger-core/src/reversal.rs` | `tests/invariants.rs::a_reversal_that_is_not_the_mirror_is_refused` |
| Hash chain | `rust/crates/kernel/ledger-core/src/hash_chain.rs` | `tests/invariants.rs::the_hash_chain_breaks_when_a_posted_line_is_altered` |
| Exact amounts | `rust/crates/common/src/money.rs`, `double_entry.rs` | `tests/invariants.rs::an_exact_comparison_refuses_a_difference_a_float_would_swallow` |
| Safe Rust | `#![forbid(unsafe_code)]` in every kernel crate | `tests/invariants.rs::every_kernel_crate_forbids_unsafe_code` |
| Parity with the retired services | `rust/tests/fixtures/parity/` | `tests/parity.rs` |
