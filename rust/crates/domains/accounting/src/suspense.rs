#![forbid(unsafe_code)]
//! Suspense and error, ported from suspense-error-service `/analyze`.
//!
//! Kept as shipped: what each error type does to the suspense account is a fixed
//! table, not a calculation. An omission corrects by its own amount, an
//! original-entry or complete-reversal error by twice it, and commission,
//! principle and compensating errors correct nothing at all - the last because
//! they are offsetting. Any unrecognised type corrects by its own amount.
//! The corrected balance is the suspense balance less those effects.
//!
//! Every amount is Money, so a suspense balance past double precision stays
//! exact; nothing here divides, so there is no ratio to keep.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use vimbai_common::Money;
use vimbai_store::amounts::wire;
use vimbai_store::read::{field, refused, text};
use vimbai_store::{BookScope, Graph, StoreError};

use crate::ledger_book;
use crate::subsidiary_journals::{account_for_purpose, account_name, must_be_open, positive, post};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuspenseError {
    #[serde(default)]
    pub error_type: String,
    #[serde(default)]
    pub description: String,
    pub amount: Money,
    #[serde(default)]
    pub account_affected: String,
    #[serde(default)]
    pub correct_debit: Option<String>,
    #[serde(default)]
    pub correct_credit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuspenseRequest {
    pub company_id: String,
    #[serde(default)]
    pub period: String,
    pub suspense_balance: Money,
    #[serde(default)]
    pub errors: Vec<SuspenseError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Correction {
    pub error_type: String,
    pub description: String,
    pub amount: Money,
    pub effect_on_suspense: Money,
    pub account_affected: String,
    pub corrective_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuspenseResult {
    pub company_id: String,
    pub period: String,
    pub original_suspense_balance: Money,
    pub total_errors_found: usize,
    pub corrected_suspense_balance: Money,
    pub corrections: Vec<Correction>,
}

/// What one error type does, and what the books keeper should do about it.
fn correction_for(error: &SuspenseError) -> (Money, String) {
    let amount = &error.amount;
    let twice = amount.times(&Money::parse("2").expect("two")).round(2);
    let nothing = Money::zero();
    match error.error_type.as_str() {
        "omission" => (
            amount.clone(),
            format!(
                "Record missing entry: Dr {}, Cr {}",
                error.correct_debit.as_deref().unwrap_or("N/A"),
                error.correct_credit.as_deref().unwrap_or("N/A")
            ),
        ),
        "commission" => (
            nothing,
            "Transfer from wrong to correct account".to_string(),
        ),
        "principle" => (
            nothing,
            format!(
                "Reclassify from {} to correct category",
                error.account_affected
            ),
        ),
        "original_entry" => (
            twice.clone(),
            format!(
                "Correct understated/overstated entry by {}",
                twice.to_fixed(2)
            ),
        ),
        "complete_reversal" => (
            twice,
            "Reverse and re-enter correctly (Dr/Cr swapped)".to_string(),
        ),
        "compensating" => (
            nothing,
            "Offsetting errors - no net suspense effect".to_string(),
        ),
        _ => (
            amount.clone(),
            format!("Correct entry for {}", error.description),
        ),
    }
}

/// `/analyze`
pub fn analyze_suspense(request: &SuspenseRequest) -> SuspenseResult {
    let mut corrections = Vec::with_capacity(request.errors.len());
    let mut total_corrections = Money::zero();

    for error in &request.errors {
        let (effect, action) = correction_for(error);
        total_corrections = total_corrections.plus(&effect);
        corrections.push(Correction {
            error_type: error.error_type.clone(),
            description: error.description.clone(),
            amount: error.amount.clone(),
            effect_on_suspense: effect,
            account_affected: error.account_affected.clone(),
            corrective_action: action,
        });
    }

    SuspenseResult {
        company_id: request.company_id.clone(),
        period: request.period.clone(),
        original_suspense_balance: request.suspense_balance.clone(),
        total_errors_found: request.errors.len(),
        corrected_suspense_balance: request.suspense_balance.minus(&total_corrections).round(2),
        corrections,
    }
}

/// An amount as the digits the ledger stores it as, for a reference a person may have to read.
fn as_text(amount: &Money) -> String {
    amount.to_fixed(6)
}

/// The source module a suspense clearing posts under, which the ledger keeps references unique for.
const SUSPENSE_MODULE: &str = "Suspense";

/// The account unplaced amounts sit in: the one the caller named, or the one this Book's chart calls suspense.
///
/// Its type is not asked for, because suspense is not one thing: money received and not yet identified sits there as a
/// credit, and a payment made and not yet identified sits there as a debit, so the same account can be either side of the
/// books from one month to the next.
fn suspense_account<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<String, StoreError> {
    // suspense is a purpose like any other: the Book says which account keeps it, and the caller may name one instead
    account_for_purpose(
        graph,
        scope,
        &text(payload, "suspense_account"),
        "suspense",
        "",
    )
}

/// Which side of the account the amount belongs on. Suspense takes the other side, because clearing it is moving the
/// amount out of suspense and into the account it was always meant for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Debit,
    Credit,
}

impl Side {
    fn of(payload: &Value) -> Result<Self, StoreError> {
        let said = text(payload, "side").trim().to_lowercase();
        match said.as_str() {
            "debit" | "dr" => Ok(Side::Debit),
            "credit" | "cr" => Ok(Side::Credit),
            "" => Err(refused(
                "Say which side of the account the amount belongs on: side is debit or credit. Suspense takes the other side.",
            )),
            other => Err(refused(&format!(
                "{other} does not say which side. Say debit or credit."
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Side::Debit => "debit",
            Side::Credit => "credit",
        }
    }
}

/// `POST /suspense/clear`: move an amount that could not be placed out of suspense and into the account it belongs in.
///
/// This is the posting half of the suspense journal. The analysis above says what a kind of error does to a suspense
/// balance; this says what clearing one actually does to the books, which is one entry: the account that should have
/// carried the amount takes its side, and suspense gives it up.
///
/// What this keeps, and why:
///
/// * the amount cleared is stated by a person, not computed from an error table: the analysis is advice about what an
///   error is worth, and what actually goes into the books is the figure the books keeper has decided to move;
/// * the entry is one clearings worth of work, and its reference names the day, the account and the amount, so pressing
///   the same clearing twice is refused by the ledger rather than moving the same amount out of suspense twice, while a
///   second, different amount cleared to the same account on the same day is its own entry;
/// * a closed month is not written into: its profit has been reported;
/// * clearing into the suspense account itself is refused, because that moves nothing and would look like work done;
/// * nothing moves money. Suspense is a bookkeeping place, not an account anybody holds.
pub fn clear_suspense<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let entry_date = text(payload, "entry_date").trim().to_string();
    if entry_date.is_empty() {
        return Err(refused(
            "Say the day the clearing belongs to, as 2026-11-20, in entry_date.",
        ));
    }
    must_be_open(graph, scope, &entry_date)?;
    let amount = positive(payload, "amount", "A suspense clearing")?;
    let side = Side::of(payload)?;
    let suspense = suspense_account(graph, scope, payload)?;

    let accounts = ledger_book::accounts(graph, scope)?;
    let into = text(payload, "account_number").trim().to_string();
    if into.is_empty() {
        return Err(refused(
            "Say which account the amount belongs in: account_number.",
        ));
    }
    let account = accounts
        .iter()
        .find(|account| field(account, "account_number") == into)
        .ok_or_else(|| {
            refused(&format!(
                "Account {into} is not in this Book's chart, so nothing can be posted to it."
            ))
        })?;
    if into == suspense {
        return Err(refused(
            "Clearing suspense into itself moves nothing: say the account the amount belongs in.",
        ));
    }

    let why = text(payload, "reason");
    let named = account_name(account);
    let description = if why.trim().is_empty() {
        format!("Suspense cleared: {named}")
    } else {
        format!("Suspense cleared: {named} ({why})")
    };
    // the day, the account and the amount: the same clearing twice is refused, a different one is its own entry
    let reference = match text(payload, "reference").trim() {
        // wire() is the JSON string the ledger stores, so the reference takes its text, not its quotes
        "" => format!("SUS-{entry_date}-{into}-{}", as_text(&amount)),
        given => given.to_string(),
    };
    let (debit, credit) = match side {
        Side::Debit => (&into, &suspense),
        Side::Credit => (&suspense, &into),
    };
    let note = if why.trim().is_empty() {
        format!("Cleared out of suspense into {named}")
    } else {
        format!("Cleared out of suspense into {named}: {why}")
    };
    let entry_id = post(
        graph,
        scope,
        payload,
        description.clone(),
        &reference,
        SUSPENSE_MODULE,
        debit,
        credit,
        &amount,
        &note,
    )?;

    Ok(json!({
        "entry_date": entry_date,
        "amount": wire(&amount),
        "side": side.as_str(),
        "account_number": into,
        "account_name": named,
        "suspense_account": suspense,
        "description": description,
        "reference": reference,
        "journal_entry_id": entry_id,
    }))
}

/// `suspense-error-service /analyze`.
pub fn suspense_error_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: SuspenseRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(analyze_suspense(&request)).map_err(|error| error.to_string())
}

/// A Book with a chart that carries a suspense account, for the tests that post.
#[cfg(test)]
mod tests_support {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    pub(super) fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    /// A Book as it opens, which already keeps an account for what cannot yet be explained: a business that needs
    /// suspense does not have to create one before it can record a difference.
    pub(super) fn book() -> (MemoryGraph, String) {
        let graph = MemoryGraph::new();
        crate::chart::seed_opening_chart(&graph, &scope(), "business").expect("a chart");
        (graph, "1150".to_string())
    }

    pub(super) fn entry(graph: &MemoryGraph, entry_id: &str) -> Value {
        crate::ledger_book::get_entry(graph, &scope(), entry_id).expect("the entry")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn fixed(amount: &Money) -> String {
        amount.to_fixed(2)
    }

    fn error(kind: &str, amount: &str, account: &str) -> SuspenseError {
        SuspenseError {
            error_type: kind.to_string(),
            description: format!("{kind} found"),
            amount: m(amount),
            account_affected: account.to_string(),
            correct_debit: Some("Purchases".to_string()),
            correct_credit: Some("Suspense".to_string()),
        }
    }

    fn request() -> SuspenseRequest {
        SuspenseRequest {
            company_id: "c1".to_string(),
            period: "2026-11".to_string(),
            suspense_balance: m("1000"),
            errors: vec![
                error("omission", "150", "Sales"),
                error("commission", "200", "Rent"),
                error("original_entry", "50", "Wages"),
            ],
        }
    }

    #[test]
    fn each_error_type_moves_the_balance_by_its_fixed_table() {
        let out = analyze_suspense(&request());
        assert_eq!(out.total_errors_found, 3);
        assert_eq!(fixed(&out.corrections[0].effect_on_suspense), "150.00");
        assert_eq!(fixed(&out.corrections[1].effect_on_suspense), "0.00");
        assert_eq!(fixed(&out.corrections[2].effect_on_suspense), "100.00");
        // 1000 - (150 + 0 + 100)
        assert_eq!(fixed(&out.corrected_suspense_balance), "750.00");
        assert_eq!(fixed(&out.original_suspense_balance), "1000.00");
    }

    #[test]
    fn the_actions_name_the_accounts_as_the_service_did() {
        let out = analyze_suspense(&request());
        assert_eq!(
            out.corrections[0].corrective_action,
            "Record missing entry: Dr Purchases, Cr Suspense"
        );
        assert_eq!(
            out.corrections[1].corrective_action,
            "Transfer from wrong to correct account"
        );
        assert_eq!(
            out.corrections[2].corrective_action,
            "Correct understated/overstated entry by 100.00"
        );
    }

    #[test]
    fn compensating_principle_and_reversal_keep_their_shape() {
        let out = analyze_suspense(&SuspenseRequest {
            company_id: "c1".to_string(),
            period: "2026-11".to_string(),
            suspense_balance: m("500"),
            errors: vec![
                error("principle", "80", "Motor"),
                error("compensating", "90", "Sundry"),
                error("complete_reversal", "25", "Bank"),
                error("something-new", "40", "Other"),
            ],
        });
        assert_eq!(fixed(&out.corrections[0].effect_on_suspense), "0.00");
        assert_eq!(
            out.corrections[0].corrective_action,
            "Reclassify from Motor to correct category"
        );
        assert_eq!(
            out.corrections[1].corrective_action,
            "Offsetting errors - no net suspense effect"
        );
        assert_eq!(fixed(&out.corrections[2].effect_on_suspense), "50.00");
        assert_eq!(
            out.corrections[2].corrective_action,
            "Reverse and re-enter correctly (Dr/Cr swapped)"
        );
        // an unknown type corrects by its own amount
        assert_eq!(fixed(&out.corrections[3].effect_on_suspense), "40.00");
        assert_eq!(
            out.corrections[3].corrective_action,
            "Correct entry for something-new found"
        );
        assert_eq!(fixed(&out.corrected_suspense_balance), "410.00");
    }

    #[test]
    fn a_missing_account_falls_back_to_not_available() {
        let mut without = error("omission", "10", "Sales");
        without.correct_debit = None;
        without.correct_credit = None;
        let out = analyze_suspense(&SuspenseRequest {
            company_id: "c1".to_string(),
            period: String::new(),
            suspense_balance: m("10"),
            errors: vec![without],
        });
        assert_eq!(
            out.corrections[0].corrective_action,
            "Record missing entry: Dr N/A, Cr N/A"
        );
    }

    #[test]
    fn a_balance_past_double_precision_stays_exact() {
        let out = analyze_suspense(&SuspenseRequest {
            company_id: "c1".to_string(),
            period: "2026-11".to_string(),
            suspense_balance: m("123456789012345678901.55"),
            errors: vec![error("original_entry", "0.55", "Wages")],
        });
        // twice 0.55 comes off the balance, digit for digit
        assert_eq!(fixed(&out.corrections[0].effect_on_suspense), "1.10");
        assert_eq!(
            fixed(&out.corrected_suspense_balance),
            "123456789012345678900.45"
        );
    }

    #[test]
    fn the_door_reads_the_published_shape() {
        let payload = serde_json::json!({
            "company_id": "c1", "period": "2026-11", "suspense_balance": "1000",
            "errors": [
                {"error_type": "omission", "description": "d", "amount": "150",
                 "account_affected": "Sales", "correct_debit": "Purchases",
                 "correct_credit": "Suspense"},
                {"error_type": "commission", "description": "d", "amount": "200",
                 "account_affected": "Rent"}
            ]
        });
        let out = suspense_error_from_json(&payload).expect("door");
        assert_eq!(out["corrected_suspense_balance"], "850.00");
        assert_eq!(out["total_errors_found"], 2);

        let bad = suspense_error_from_json(&serde_json::json!({
            "company_id": "c1", "suspense_balance": "lots"
        }))
        .expect_err("refused");
        assert!(bad.contains("invalid request"), "{bad}");
    }
}

#[cfg(test)]
mod clearing_tests {
    use super::tests_support::*;
    use super::*;

    fn clearing(payload: Value) -> Result<Value, StoreError> {
        let (graph, suspense) = book();
        let answer = clear_suspense(&graph, &scope(), &payload)?;
        assert_eq!(answer["suspense_account"], json!(suspense));
        Ok(answer)
    }

    #[test]
    fn an_amount_credited_where_it_belongs_comes_out_of_suspense() {
        let (graph, suspense) = book();
        let answer = clear_suspense(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-20", "amount": "500", "side": "credit",
                    "account_number": "4000", "reason": "customer identified"}),
        )
        .expect("cleared");

        // the business earned 500 and suspense no longer holds it: Dr Suspense, Cr Sales
        let entry = entry(&graph, field(&answer, "journal_entry_id"));
        assert_eq!(entry["entry_date"], json!("2026-11-20"));
        assert_eq!(field(&entry, "source_module"), "Suspense");
        let lines = entry["lines"].as_array().expect("lines");
        assert_eq!(lines.len(), 2);
        assert_eq!(field(&lines[0], "account_number"), suspense);
        assert_eq!(lines[0]["debit"], json!("500.000000"));
        assert_eq!(field(&lines[1], "account_number"), "4000");
        assert_eq!(lines[1]["credit"], json!("500.000000"));
    }

    #[test]
    fn an_amount_debited_where_it_belongs_takes_the_other_side() {
        let (graph, suspense) = book();
        let answer = clear_suspense(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-20", "amount": "80", "side": "debit",
                    "account_number": "5000"}),
        )
        .expect("cleared");
        let entry = entry(&graph, field(&answer, "journal_entry_id"));
        let lines = entry["lines"].as_array().expect("lines");
        // the expense is charged and suspense gives up what it was holding: Dr Expenses, Cr Suspense
        assert_eq!(field(&lines[0], "account_number"), "5000");
        assert_eq!(lines[0]["debit"], json!("80.000000"));
        assert_eq!(field(&lines[1], "account_number"), suspense);
        assert_eq!(lines[1]["credit"], json!("80.000000"));
    }

    #[test]
    fn the_same_clearing_twice_is_refused_by_the_ledger() {
        let (graph, _suspense) = book();
        let payload = json!({"entry_date": "2026-11-20", "amount": "500", "side": "credit",
                             "account_number": "4000"});
        clear_suspense(&graph, &scope(), &payload).expect("cleared");
        let refused = clear_suspense(&graph, &scope(), &payload).expect_err("the same clearing");
        let said = refused.to_string();
        assert!(said.contains("already exists"), "{said}");
    }

    #[test]
    fn a_second_different_amount_cleared_the_same_day_is_its_own_entry() {
        let (graph, _suspense) = book();
        clear_suspense(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-20", "amount": "500", "side": "credit",
                    "account_number": "4000"}),
        )
        .expect("first");
        let second = clear_suspense(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-20", "amount": "250", "side": "credit",
                    "account_number": "4000"}),
        )
        .expect("second");
        assert_eq!(second["amount"], json!("250.000000"));
        assert_ne!(second["journal_entry_id"], Value::Null);
    }

    #[test]
    fn clearing_into_suspense_itself_is_refused_in_words() {
        let (graph, suspense) = book();
        let refused = clear_suspense(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-20", "amount": "10", "side": "credit",
                    "account_number": suspense}),
        )
        .expect_err("into itself");
        assert!(refused.to_string().contains("itself"), "{refused}");
    }

    #[test]
    fn a_side_a_day_and_an_account_are_asked_for_when_missing() {
        let (graph, _suspense) = book();
        for (payload, wanted) in [
            (
                json!({"amount": "10", "side": "credit", "account_number": "4000"}),
                "entry_date",
            ),
            (
                json!({"entry_date": "2026-11-20", "amount": "10", "account_number": "4000"}),
                "side is debit or credit",
            ),
            (
                json!({"entry_date": "2026-11-20", "amount": "10", "side": "credit"}),
                "account_number",
            ),
        ] {
            let refused = clear_suspense(&graph, &scope(), &payload).expect_err("asked for");
            assert!(refused.to_string().contains(wanted), "{refused}");
        }
    }

    #[test]
    fn an_amount_of_nothing_is_refused() {
        let (graph, _suspense) = book();
        let refused = clear_suspense(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-20", "amount": "0", "side": "credit",
                    "account_number": "4000"}),
        )
        .expect_err("nothing");
        assert!(
            refused.to_string().contains("more than nothing"),
            "{refused}"
        );
    }

    #[test]
    fn a_closed_month_is_not_written_into() {
        let (graph, _suspense) = book();
        crate::periods::close_period(
            &graph,
            &scope(),
            &json!({"period": "2026-03", "closes_on": "2026-03-31"}),
        )
        .expect("a closed month");
        let refused = clear_suspense(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-03-15", "amount": "10", "side": "credit",
                    "account_number": "4000"}),
        )
        .expect_err("a closed month");
        let said = refused.to_string();
        assert!(said.contains("closed") || said.contains("open"), "{said}");
    }

    #[test]
    fn a_book_with_no_suspense_account_is_told_which_setting_to_say() {
        // a Book whose chart holds no suspense account says so rather than posting somewhere else
        let graph = vimbai_store::fake::MemoryGraph::new();
        crate::ledger_book::create_account(
            &graph,
            &scope(),
            &json!({
                "account_number": "4000",
                "name": "Sales",
                "account_type": "revenue",
                "opening_balance": "0",
            }),
        )
        .expect("a sales account");
        let refused = clear_suspense(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-20", "amount": "10", "side": "credit",
                    "account_number": "4000"}),
        )
        .expect_err("no suspense account");
        let said = refused.to_string();
        assert!(said.contains("suspense"), "{said}");
    }

    #[test]
    fn an_amount_past_double_precision_clears_digit_for_digit() {
        let (graph, _suspense) = book();
        let answer = clear_suspense(
            &graph,
            &scope(),
            &json!({"entry_date": "2026-11-20", "amount": "123456789012345678901.55",
                    "side": "credit", "account_number": "4000"}),
        )
        .expect("cleared");
        let entry = entry(&graph, field(&answer, "journal_entry_id"));
        assert_eq!(
            entry["lines"].as_array().expect("lines")[0]["debit"],
            json!("123456789012345678901.550000")
        );
    }

    #[test]
    fn a_clearing_reports_what_it_did() {
        let answer = clearing(
            json!({"entry_date": "2026-11-20", "amount": "500", "side": "credit",
                                     "account_number": "4000", "reason": "customer identified"}),
        )
        .expect("cleared");
        assert_eq!(answer["side"], json!("credit"));
        assert_eq!(answer["account_number"], json!("4000"));
        assert_eq!(
            answer["description"],
            json!("Suspense cleared: Sales (customer identified)")
        );
        assert_eq!(answer["reference"], json!("SUS-2026-11-20-4000-500.000000"));
    }
}
