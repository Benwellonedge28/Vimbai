//! The five invariants ledger-core owns, proven from outside ledger-core.
//!
//! Nothing here reaches into the kernel's internals: every one of these walks the public surface a domain calls. That is
//! the point of the crate - if a rule stops holding, the test that names it fails, and the failure is not in the same
//! file as the code it accuses.
//!
//! 1. `an_entry_balances_or_is_refused` - debits equal credits, or the entry does not exist
//! 2. `a_posted_entry_cannot_be_edited_or_deleted` - a correction is a reversing entry, not an edit
//! 3. `a_reversal_that_is_not_the_mirror_is_refused` and `a_reversal_nets_the_original_to_zero`
//! 4. `the_hash_chain_breaks_when_a_posted_line_is_altered` - tamper evidence, not tamper prevention dressed up
//! 5. `an_exact_comparison_refuses_a_difference_a_float_would_swallow` - no float enters the comparison path

use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_ledger_core::double_entry::ValidationReport;
use vimbai_ledger_core::reversal::{flatten_lines, AccountMovement};
use vimbai_ledger_core::{
    chain_state_from_digests, check_reversal, compute_entry_hash, validate_lines,
    validate_money_lines, EntryDigest, JournalLine,
};
use vimbai_service_accounting::ledger_book;
use vimbai_store::fake::MemoryGraph;
use vimbai_store::{BookScope, Records};

fn scope() -> BookScope {
    BookScope::new("user-1", "book-1")
}

fn line(account: &str, debit: &str, credit: &str) -> Value {
    json!({ "account_number": account, "debit": debit, "credit": credit })
}

fn entry(status: &str, lines: Value) -> Value {
    json!({ "entry_date": "2026-03-15", "description": "a sale", "status": status, "lines": lines })
}

/// A Book with a small chart, because an entry may only move accounts its own Book owns.
fn charted() -> MemoryGraph {
    let graph = MemoryGraph::new();
    for (number, kind, normal) in [
        ("1000", "asset", "debit"),
        ("4000", "revenue", "credit"),
        ("5000", "expense", "debit"),
    ] {
        Records::new(&graph, vimbai_store::LEDGER_ACCOUNT)
            .create(
                &scope(),
                &json!({ "account_number": number, "name": format!("{kind} {number}"),
                         "account_type": kind, "normal_balance": normal }),
            )
            .expect("an account");
    }
    graph
}

fn valid(report: &ValidationReport) -> bool {
    report.valid
}

#[test]
fn an_entry_balances_or_is_refused() {
    // two lines that agree
    let balanced = validate_lines(&[JournalLine::new(250.0, 0.0), JournalLine::new(0.0, 250.0)]);
    assert!(valid(&balanced), "{balanced:?}");

    // two that do not: 250 in, 240 out
    let unbalanced = validate_lines(&[JournalLine::new(250.0, 0.0), JournalLine::new(0.0, 240.0)]);
    assert!(!valid(&unbalanced), "a difference must be refused");
    assert!(!unbalanced.errors.is_empty());

    // a line that moves money both ways at once, and a line that moves none
    let both_ways = validate_lines(&[JournalLine::new(10.0, 10.0), JournalLine::new(10.0, 10.0)]);
    assert!(!valid(&both_ways), "a line cannot be a debit and a credit");
    let empty = validate_lines(&[JournalLine::new(0.0, 0.0), JournalLine::new(0.0, 0.0)]);
    assert!(!valid(&empty), "a line must move something");

    // and the domain above the kernel refuses the same entry before it is ever stamped
    let graph = charted();
    let refused = ledger_book::create_entry(
        &graph,
        &scope(),
        &entry(
            "posted",
            json!([line("1000", "250.00", "0"), line("4000", "0", "240.00")]),
        ),
    );
    assert!(refused.is_err(), "a Book must not hold an unbalanced entry");
    // ... so nothing was written
    assert!(
        graph.held("book-1", "JournalEntry").is_empty(),
        "nothing was written"
    );
}

#[test]
fn a_posted_entry_cannot_be_edited_or_deleted() {
    let graph = charted();
    let posted = ledger_book::create_entry(
        &graph,
        &scope(),
        &entry(
            "posted",
            json!([line("1000", "250.00", "0"), line("4000", "0", "250.00")]),
        ),
    )
    .expect("a balanced entry is accepted");
    let id = posted["id"].as_str().expect("an id").to_string();

    // editing it is refused, whichever stamped field is touched
    let edited = ledger_book::update_entry(
        &graph,
        &scope(),
        &id,
        &json!({ "description": "something else" }),
    );
    assert!(
        edited.is_err(),
        "a posted entry is not editable: {edited:?}"
    );

    // deleting it is refused
    let deleted = ledger_book::delete_entry(&graph, &scope(), &id);
    assert!(
        deleted.is_err(),
        "a posted entry is not deletable: {deleted:?}"
    );

    // and it is still exactly what it was
    let still = ledger_book::get_entry(&graph, &scope(), &id).expect("still there");
    assert_eq!(still["status"], json!("posted"));
    assert_eq!(still["description"], json!("a sale"));

    // a draft, on the other hand, is the working copy the rule is protecting: it may go
    let draft = ledger_book::create_entry(
        &graph,
        &scope(),
        &entry(
            "draft",
            json!([line("5000", "10.00", "0"), line("1000", "0", "10.00")]),
        ),
    )
    .expect("a draft");
    let draft_id = draft["id"].as_str().expect("an id").to_string();
    ledger_book::delete_entry(&graph, &scope(), &draft_id).expect("a draft may be discarded");
}

fn movements(account: &str, amount: f64, debit: bool) -> AccountMovement {
    AccountMovement {
        account: account.to_string(),
        debit: if debit { amount } else { 0.0 },
        credit: if debit { 0.0 } else { amount },
    }
}

#[test]
fn a_reversal_that_is_not_the_mirror_is_refused() {
    let original = vec![
        movements("1000", 250.0, true),
        movements("4000", 250.0, false),
    ];

    // the mirror: same accounts, sides swapped
    let mirrored = vec![
        movements("1000", 250.0, false),
        movements("4000", 250.0, true),
    ];
    let accepted = check_reversal(&original, &mirrored);
    assert!(accepted.valid, "{accepted:?}");

    // a different amount is not a reversal, it is a second entry wearing the same accounts
    let wrong_amount = vec![
        movements("1000", 240.0, false),
        movements("4000", 240.0, true),
    ];
    assert!(!check_reversal(&original, &wrong_amount).valid);

    // an account the original did not touch is not a reversal either
    let extra_account = vec![
        movements("1000", 250.0, false),
        movements("4000", 250.0, true),
        movements("5000", 1.0, true),
    ];
    assert!(!check_reversal(&original, &extra_account).valid);

    // a partial mirror - one account flipped, one not - is the tempting one, and it is refused
    let half = vec![
        movements("1000", 250.0, false),
        movements("4000", 250.0, false),
    ];
    assert!(!check_reversal(&original, &half).valid);
}

#[test]
fn a_reversal_nets_the_original_to_zero() {
    let accounts = ["1000".to_string(), "4000".to_string(), "5000".to_string()];
    // an entry moving three accounts: 250 out of the bank, 150 of it revenue and 100 of it an expense correction
    let original = vec![
        movements("1000", 250.0, true),
        movements("4000", 150.0, false),
        movements("5000", 100.0, false),
    ];
    let reversal: Vec<AccountMovement> = original
        .iter()
        .map(|move_| AccountMovement {
            account: move_.account.clone(),
            debit: move_.credit,
            credit: move_.debit,
        })
        .collect();
    // flattening a written entry and its mirror must give the same shape back
    let written = vec![
        JournalLine::new(250.0, 0.0),
        JournalLine::new(0.0, 150.0),
        JournalLine::new(0.0, 100.0),
    ];
    assert_eq!(flatten_lines(&written, &accounts), original);

    assert!(check_reversal(&original, &reversal).valid);

    // every account the pair touched ends at exactly zero, not at "close enough to zero"
    for account in &accounts {
        let net: Money = original
            .iter()
            .chain(reversal.iter())
            .filter(|move_| &move_.account == account)
            .fold(Money::zero(), |running, move_| {
                if move_.debit > 0.0 {
                    running + Money::from_f64_shortest(move_.debit).expect("an amount")
                } else {
                    running - Money::from_f64_shortest(move_.credit).expect("an amount")
                }
            });
        assert_eq!(net, Money::zero(), "account {account} nets to {net}");
    }
}

#[test]
fn the_hash_chain_breaks_when_a_posted_line_is_altered() {
    // three entries, each stamped over the one before it, exactly as the ledger writer records them
    fn ledger_payload(description: &str, amount: &str) -> Value {
        json!({ "entry_date": "2026-03-15", "description": description,
                "lines": [json!({"account_number": "1000", "debit": amount, "credit": "0"}),
                          json!({"account_number": "4000", "debit": "0", "credit": amount})] })
    }
    fn digest(id: &str, payload: Value, prev: &str) -> EntryDigest {
        let stamp = compute_entry_hash(prev, &payload).expect("a stamp");
        EntryDigest {
            entry_id: id.to_string(),
            payload: payload.as_object().expect("an object").clone(),
            prev_hash: prev.to_string(),
            stored_hash: Some(stamp),
        }
    }

    let genesis = vimbai_ledger_core::GENESIS_HASH.to_string();
    let first = digest("entry-1", ledger_payload("first", "250.00"), &genesis);
    let second = digest(
        "entry-2",
        ledger_payload("second", "100.00"),
        &first.stored_hash.clone().unwrap(),
    );
    let third = digest(
        "entry-3",
        ledger_payload("third", "50.00"),
        &second.stored_hash.clone().unwrap(),
    );
    let chain = vec![first.clone(), second.clone(), third];

    let intact = chain_state_from_digests(&chain);
    assert!(intact.valid, "an untouched chain must verify: {intact:?}");
    assert_eq!(intact.entries_checked, 3);

    // somebody edits the middle entry's amount and leaves the stamp it was recorded with
    let mut tampered = second.clone();
    tampered.payload = ledger_payload("second", "900.00")
        .as_object()
        .expect("an object")
        .clone();
    let detected = chain_state_from_digests(&[first.clone(), tampered.clone(), chain[2].clone()]);
    assert!(
        !detected.valid,
        "an altered line must fail its own stamp: {detected:?}"
    );
    assert!(
        detected
            .errors
            .iter()
            .any(|error| format!("{error:?}").contains("entry-2")),
        "the break must name the entry: {detected:?}"
    );

    // and re-stamping the edited entry breaks the entry after it, because that one chained over the old stamp
    let mut restamped = tampered;
    restamped.stored_hash = Some(
        compute_entry_hash(
            &first.stored_hash.clone().unwrap(),
            &Value::Object(restamped.payload.clone()),
        )
        .expect("a stamp"),
    );
    let relinked = chain_state_from_digests(&[first.clone(), restamped.clone(), chain[2].clone()]);
    assert!(
        !relinked.valid,
        "the next entry still names the old stamp: {relinked:?}"
    );

    // a wholly re-stamped chain verifies again - the guarantee is tamper evidence, not tamper prevention, and this is
    // what it means: changing history is detectable unless every later stamp is recomputed too
    let repaired_second_stamp = restamped.stored_hash.clone().unwrap();
    let repaired_third = digest(
        "entry-3",
        ledger_payload("third", "50.00"),
        &repaired_second_stamp,
    );
    let repaired = chain_state_from_digests(&[first, restamped, repaired_third]);
    assert!(
        repaired.valid,
        "a chain rebuilt end to end verifies: {repaired:?}"
    );
}

#[test]
fn an_exact_comparison_refuses_a_difference_a_float_would_swallow() {
    // 0.1 + 0.2 against 0.3 is the classic float trap: the doubles do not add up
    let floats_do_not_agree = 0.1_f64 + 0.2_f64 != 0.3_f64;
    assert!(floats_do_not_agree, "the trap this test exists to show");

    // the exact path has no such trouble
    let exact = validate_money_lines(&[
        (Money::parse("0.1").expect("an amount"), Money::zero()),
        (Money::parse("0.2").expect("an amount"), Money::zero()),
        (Money::zero(), Money::parse("0.3").expect("an amount")),
    ]);
    assert!(valid(&exact), "{exact:?}");

    // and a difference of one unit in the thirtieth decimal place is refused, where a float comparison would have
    // called the two sides equal and posted an entry that does not balance
    let off_by_a_whisker = validate_money_lines(&[
        (
            Money::parse("0.300000000000000000000000000001").expect("an amount"),
            Money::zero(),
        ),
        (Money::zero(), Money::parse("0.3").expect("an amount")),
    ]);
    assert!(
        !valid(&off_by_a_whisker),
        "an exact comparison must refuse a one-digit difference: {off_by_a_whisker:?}"
    );

    // the check is not brute force: an amount far beyond f64's 15 digits balances exactly
    let large = "123456789012345678901234567890.123456789";
    let mirror = validate_money_lines(&[
        (Money::parse(large).expect("an amount"), Money::zero()),
        (Money::zero(), Money::parse(large).expect("an amount")),
    ]);
    assert!(valid(&mirror), "{mirror:?}");
}

#[test]
fn every_kernel_crate_forbids_unsafe_code() {
    // the promise is the attribute, not a sentence in a comment: this fails the day somebody deletes it
    let kernel = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/kernel");
    let mut checked = 0;
    for crate_dir in std::fs::read_dir(&kernel).expect("the kernel") {
        let crate_dir = crate_dir.expect("an entry").path();
        let lib = crate_dir.join("src/lib.rs");
        if !lib.exists() {
            continue;
        }
        let source = std::fs::read_to_string(&lib).expect("lib.rs");
        assert!(
            source.contains("#![forbid(unsafe_code)]"),
            "{} must forbid unsafe code",
            lib.display()
        );
        assert!(
            !source.contains("unsafe {") && !source.contains("unsafe fn"),
            "{} must contain no unsafe block",
            lib.display()
        );
        checked += 1;
    }
    assert!(checked >= 2, "the kernel is more than one crate: {checked}");
}
