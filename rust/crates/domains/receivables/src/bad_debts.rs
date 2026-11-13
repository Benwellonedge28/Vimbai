#![forbid(unsafe_code)]
//! bad-debts-recovery-service: debts written off, and what has come back since.
//!
//! A written-off debt is a record with what was owed, what was given up on and what has since been
//! recovered. A recovery adds to the third and moves the status, and each recovery leaves its own journal
//! beside the debt so the money that came back can be traced.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a recovery against a debt that is already fully recovered is refused rather than banked twice;
//! * the status turns to completed once the recovered total reaches the amount written off - a partial
//!   recovery stays partial even if it is over the outstanding figure, because the test is against what was
//!   written off, not against what was left;
//! * the summary's pending figure counts only the debts that are not complete, so a completed debt
//!   contributes nothing to what is still owed;
//! * the recovery rate is a percentage of what was written off, and is nil rather than a division when
//!   nothing was written off at all.

use serde_json::{json, Value};

use crate::rates::name_of;
use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, BAD_DEBT_RECORD, RECOVERY_JOURNAL};

const PENDING: &str = "pending";
const PARTIAL: &str = "partial";
const COMPLETED: &str = "completed";

fn debt_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debt_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, BAD_DEBT_RECORD).get(scope, debt_id)
}

/// `POST /bad-debts/register`.
pub fn register_debt(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let debtor = text(payload, "debtor_name");
    if debtor.is_empty() {
        return Err(refused("debtor_name is required"));
    }
    let written_off = amount_or_zero(payload, "written_off_amount")?;

    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("id");
        fields.insert("recovered_amount".into(), wire(&Money::zero()));
        fields.insert("status".into(), json!(PENDING));
        // a debt is registered as wholly unrecovered, whatever the caller says
        let _ = written_off;
    } else {
        return Err(refused("a bad debt must be an object"));
    }
    remember(graph, scope, BAD_DEBT_RECORD, body)
}

/// `GET /bad-debts/{debt_id}`.
pub fn get_bad_debt(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debt_id: &str,
) -> Result<Value, StoreError> {
    debt_of(graph, scope, debt_id)
}

/// `GET /bad-debts`.
pub fn list_bad_debts(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let debts = Records::new(graph, BAD_DEBT_RECORD).list(scope, 2000)?;
    Ok(json!({"bad_debts": debts, "count": debts.len()}))
}

/// `POST /bad-debts/{debt_id}/recover`.
pub fn record_recovery(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debt_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let debt = debt_of(graph, scope, debt_id)?;
    if field(&debt, "status") == COMPLETED {
        return Err(refused("recovery already completed"));
    }
    let amount = amount_or_zero(payload, "amount")?;
    if amount.is_zero() {
        return Err(refused("amount is required"));
    }

    let recovered = stored_amount(&debt, "recovered_amount")
        .unwrap_or_else(|_| Money::zero())
        .plus(&amount);
    let written_off = stored_amount(&debt, "written_off_amount").unwrap_or_else(|_| Money::zero());
    // compared as exact decimals: turning either side into a float first could call a debt complete a
    // hair early, and the amounts here are the amounts the ledger was written off at
    let status = if recovered.is_zero() || recovered < written_off {
        PARTIAL
    } else {
        COMPLETED
    };

    let journal = remember(
        graph,
        scope,
        RECOVERY_JOURNAL,
        json!({
            "recovery_id": debt_id,
            "amount": wire(&amount),
            "date": text(payload, "recovery_date"),
            "description": format!("Recovery from {}", name_of(&debt)),
        }),
    )?;

    Records::new(graph, BAD_DEBT_RECORD).update(
        scope,
        debt_id,
        &json!({
            "recovered_amount": wire(&recovered),
            "recovery_date": text(payload, "recovery_date"),
            "payment_method": text(payload, "payment_method"),
            "reference": text(payload, "reference"),
            "status": status,
        }),
    )?;

    Ok(json!({"debt": debt_of(graph, scope, debt_id)?, "journal": journal}))
}

/// `GET /recovery-summary`.
pub fn recovery_summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let debts = Records::new(graph, BAD_DEBT_RECORD).list(scope, 2000)?;
    summarise(&debts)
}

fn summarise(debts: &[Value]) -> Result<Value, StoreError> {
    let mut written_off = Money::zero();
    let mut recovered = Money::zero();
    let mut outstanding = Money::zero();
    let (mut pending, mut partial, mut completed) = (0usize, 0usize, 0usize);

    for debt in debts {
        let owed = stored_amount(debt, "written_off_amount").unwrap_or_else(|_| Money::zero());
        let back = stored_amount(debt, "recovered_amount").unwrap_or_else(|_| Money::zero());
        written_off = written_off.plus(&owed);
        recovered = recovered.plus(&back);
        match field(debt, "status") {
            COMPLETED => completed += 1,
            PARTIAL => {
                partial += 1;
                outstanding = outstanding.plus(&owed.minus(&back));
            }
            _ => {
                pending += 1;
                outstanding = outstanding.plus(&owed.minus(&back));
            }
        }
    }

    Ok(json!({
        "total_bad_debts": debts.len(),
        "total_written_off": wire(&written_off),
        "total_recovered": wire(&recovered),
        "pending_recovery": wire(&outstanding),
        "recovery_rate": wire(&recovered.ratio_pct(&written_off, 2)),
        "status_breakdown": {"pending": pending, "partial": partial, "completed": completed},
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn a_newly_written_off_debt_is_unrecovered_and_pending() {
        let graph = FakeGraph::returning(vec![json!({"id": "b-1"})]);
        let answer = register_debt(
            &graph,
            &scope(),
            &json!({"original_debt_id": "d-1", "debtor_name": "Ana", "original_amount": 5000,
                    "written_off_amount": 5000}),
        )
        .expect("registered");
        assert_eq!(answer["status"], json!("pending"));
        assert_eq!(answer["recovered_amount"], json!("0.000000"));
    }

    #[test]
    fn a_partial_recovery_stays_partial_until_the_write_off_is_matched() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "b-1", "debtor_name": "Ana", "written_off_amount": "5000.000000",
                        "recovered_amount": "1000.000000", "status": "partial"}),
            ],
            vec![json!({"id": "j-1"})],
            vec![json!({"id": "b-1", "status": "partial"})],
            vec![json!({"id": "b-1", "recovered_amount": "2000.000000", "status": "partial"})],
        ]);
        let answer = record_recovery(
            &graph,
            &scope(),
            "b-1",
            &json!({"amount": 1000, "recovery_date": "2026-02-01", "payment_method": "bank"}),
        )
        .expect("recovered");
        assert_eq!(answer["debt"]["status"], json!("partial"));
        assert_eq!(answer["journal"]["amount"], json!("1000.000000"));
    }

    #[test]
    fn a_recovery_that_matches_the_write_off_completes_the_debt() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "b-1", "debtor_name": "Ana", "written_off_amount": "5000.000000",
                        "recovered_amount": "4500.000000", "status": "partial"}),
            ],
            vec![json!({"id": "j-2"})],
            vec![json!({"id": "b-1", "status": "completed"})],
            vec![json!({"id": "b-1", "status": "completed"})],
        ]);
        let answer =
            record_recovery(&graph, &scope(), "b-1", &json!({"amount": 500})).expect("recovered");
        assert_eq!(answer["debt"]["status"], json!("completed"));
    }

    #[test]
    fn a_completed_debt_is_not_banked_twice() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "b-1", "written_off_amount": "5000.000000", "recovered_amount": "5000.000000",
            "status": "completed"
        })]);
        let refused = record_recovery(&graph, &scope(), "b-1", &json!({"amount": 100}));
        assert!(matches!(refused, Err(StoreError::Invalid(_))));
    }

    #[test]
    fn the_summary_counts_only_incomplete_debts_as_still_owed() {
        let debts = vec![
            json!({"id": "b-1", "written_off_amount": "1000.000000", "recovered_amount": "250.000000", "status": "partial"}),
            json!({"id": "b-2", "written_off_amount": "500.000000", "recovered_amount": "500.000000", "status": "completed"}),
            json!({"id": "b-3", "written_off_amount": "200.000000", "recovered_amount": "0.000000", "status": "pending"}),
        ];
        let answer = summarise(&debts).expect("summarised");
        assert_eq!(answer["total_written_off"], json!("1700.000000"));
        assert_eq!(answer["total_recovered"], json!("750.000000"));
        assert_eq!(answer["pending_recovery"], json!("950.000000"));
        assert_eq!(answer["recovery_rate"], json!("44.120000"));
        assert_eq!(answer["status_breakdown"]["completed"], json!(1));
    }

    #[test]
    fn a_book_with_nothing_written_off_reports_a_rate_of_nil_rather_than_dividing() {
        let answer = summarise(&[]).expect("summarised");
        assert_eq!(answer["recovery_rate"], json!("0.000000"));
        assert_eq!(answer["total_bad_debts"], json!(0));
    }

    #[test]
    fn a_debt_the_caller_does_not_hold_is_a_miss() {
        let graph = FakeGraph::returning(vec![]);
        assert!(matches!(
            get_bad_debt(&graph, &scope(), "b-9"),
            Err(StoreError::NotFound)
        ));
    }
}
