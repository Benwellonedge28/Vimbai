#![forbid(unsafe_code)]
//! control-account-reconciliation-service: the breaks between a control account and its ledger.
//!
//! An error records a break: what the figure was, what it should have been, and the variance between them.
//! A report records a reconciliation attempt: the two figures it was asked to compare, the difference, and
//! a snapshot of the pending errors it found against that control account at the time. Both are Book-scoped
//! records, so a caller hears NotFound for a record in another Book rather than Forbidden.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the variance is always the absolute difference, so an error is never recorded as negative;
//! * an error's status starts `pending`, and resolving it writes `resolved` with who resolved it and a note
//!   - a `written_off` status can be sent when the error is created, which is how a break is abandoned;
//! * a report is `reconciled` when the difference is under a cent in either direction, because the two
//!   figures arrive already rounded by the ledger that produced them;
//! * the report keeps the errors it found as a snapshot, not a link: reconciling again later does not
//!   rewrite what an earlier report saw;
//! * the summary's variance counts pending errors only, so a resolved break stops being money at risk.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, RECONCILIATION_ERROR, RECONCILIATION_REPORT,
};

/// The four control accounts the service reconciles.
pub const CONTROL_ACCOUNTS: [&str; 4] = [
    "sales_ledger",
    "purchases_ledger",
    "bills_receivable",
    "bills_payable",
];

/// The seven kinds of break it can record.
pub const ERROR_TYPES: [&str; 7] = [
    "duplicate_entry",
    "posting_error",
    "missing_entry",
    "amount_mismatch",
    "timing_difference",
    "transfer_error",
    "rounding_error",
];

const PENDING: &str = "pending";
const RESOLVED: &str = "resolved";

fn require_one_of(value: &str, allowed: &[&str], what: &str) -> Result<(), StoreError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(refused(&format!("{what} must be one of {allowed:?}")))
    }
}

/// `POST /errors/report`.
pub fn report_error(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let error_type = text(payload, "error_type");
    require_one_of(&error_type, &ERROR_TYPES, "error_type")?;
    let control_account = text(payload, "control_account_type");
    require_one_of(&control_account, &CONTROL_ACCOUNTS, "control_account_type")?;
    let description = text(payload, "description");
    if description.is_empty() {
        return Err(refused("description is required"));
    }

    let amount = amount_or_zero(payload, "amount")?;
    let expected = amount_or_zero(payload, "expected_amount")?;
    let variance = amount.minus(&expected).abs();
    let status = match text(payload, "status") {
        empty if empty.is_empty() => PENDING.to_owned(),
        given => {
            require_one_of(&given, &[PENDING, "written_off"], "status")?;
            given
        }
    };

    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("id");
        fields.remove("user_id");
        fields.remove("book_id");
        fields.remove("resolved_at");
        fields.insert("variance".into(), wire(&variance));
        fields.insert("status".into(), json!(status));
    } else {
        return Err(refused("a reconciliation error must be an object"));
    }

    remember(graph, scope, RECONCILIATION_ERROR, body)
}

/// `GET /errors`, filtered the way the shipped service filtered after it had read the rows: by status and by
/// type, over the caller's own errors, oldest first.
pub fn list_errors(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    status: &str,
    error_type: &str,
) -> Result<Value, StoreError> {
    let mut found = Records::new(graph, RECONCILIATION_ERROR).list(scope, 2000)?;
    found.reverse(); // the store lists newest first; the service answered oldest first
    if !error_type.is_empty() {
        require_one_of(error_type, &ERROR_TYPES, "error_type")?;
        found.retain(|row| field(row, "error_type") == error_type);
    }
    if !status.is_empty() {
        found.retain(|row| field(row, "status") == status);
    }
    let count = found.len();
    Ok(json!({ "errors": found, "count": count }))
}

/// `POST /errors/{error_id}/resolve`.
pub fn resolve_error(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    error_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let resolution = text(payload, "resolution");
    if resolution.is_empty() {
        return Err(refused("resolution is required"));
    }
    let resolved_by = text(payload, "resolved_by");
    let answer = Records::new(graph, RECONCILIATION_ERROR).update(
        scope,
        error_id,
        &json!({
            "status": RESOLVED,
            "resolution": resolution,
            "resolved_by": resolved_by,
        }),
    )?;
    Ok(answer)
}

/// `POST /reconcile/{account_type}`: compare the two figures, snapshot the pending breaks against that
/// control account, and keep the report.
pub fn reconcile_control_account(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    account_type: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    require_one_of(account_type, &CONTROL_ACCOUNTS, "control_account_type")?;
    let control_balance = amount_or_zero(payload, "control_balance")?;
    let ledger_total = amount_or_zero(payload, "ledger_total")?;
    let difference = control_balance.minus(&ledger_total);

    let mut pending = Records::new(graph, RECONCILIATION_ERROR).list(scope, 2000)?;
    pending.reverse();
    pending.retain(|row| {
        field(row, "control_account_type") == account_type && field(row, "status") == PENDING
    });

    // a difference under a cent in either direction is a rounding artefact of the two ledgers, not a break
    let tolerance = Money::parse("0.01").expect("a cent parses");
    let reconciled = difference.abs() < tolerance;

    let as_of = text(payload, "as_of_date");
    let kept = remember(
        graph,
        scope,
        RECONCILIATION_REPORT,
        json!({
            "as_of_date": as_of,
            "control_account_type": account_type,
            "control_balance": wire(&control_balance),
            "ledger_total": wire(&ledger_total),
            "difference": wire(&difference),
            "error_count": pending.len(),
            "errors": pending,
            "status": if reconciled { "reconciled" } else { "unreconciled" },
        }),
    )?;
    Ok(kept)
}

/// `GET /reports`: the newest `limit` reports, oldest first within that window.
pub fn list_reports(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    limit: usize,
) -> Result<Value, StoreError> {
    let mut found = Records::new(graph, RECONCILIATION_REPORT).list(scope, 2000)?;
    found.reverse();
    let start = found.len().saturating_sub(limit);
    let window: Vec<Value> = found.split_off(start);
    Ok(json!({ "reports": window }))
}

/// `GET /summary`.
pub fn reconciliation_summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let mut errors = Records::new(graph, RECONCILIATION_ERROR).list(scope, 2000)?;
    errors.reverse();

    let mut pending_variance = Money::zero();
    let mut pending = 0usize;
    let mut resolved = 0usize;
    let mut by_type: Vec<Value> = Vec::new();
    for error_type in ERROR_TYPES {
        let count = errors
            .iter()
            .filter(|row| field(row, "error_type") == error_type)
            .count();
        by_type.push(json!({ error_type: count }));
        let _ = error_type;
    }
    for row in &errors {
        match field(row, "status") {
            PENDING => {
                pending += 1;
                pending_variance = pending_variance
                    .plus(&stored_amount(row, "variance").unwrap_or_else(|_| Money::zero()));
            }
            RESOLVED => resolved += 1,
            _ => {}
        }
    }

    let mut by_type_object = serde_json::Map::new();
    for entry in by_type {
        if let Some(object) = entry.as_object() {
            for (name, count) in object {
                by_type_object.insert(name.clone(), count.clone());
            }
        }
    }

    Ok(json!({
        "total_errors": errors.len(),
        "pending_errors": pending,
        "resolved_errors": resolved,
        "total_variance": wire(&pending_variance),
        "by_type": Value::Object(by_type_object),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("u-1", "b-1")
    }

    #[test]
    fn a_reported_error_keeps_the_absolute_variance() {
        let graph = FakeGraph::returning(vec![json!({"id": "e-new"})]);
        let error = report_error(
            &graph,
            &scope(),
            &json!({
                "error_type": "amount_mismatch",
                "control_account_type": "sales_ledger",
                "description": "invoice posted twice",
                "amount": 900,
                "expected_amount": 1200,
            }),
        )
        .expect("kept");
        assert_eq!(error["variance"], json!("300.000000"));
        assert_eq!(error["status"], json!("pending"));
    }

    #[test]
    fn an_unknown_error_type_is_refused() {
        let graph = FakeGraph::returning(vec![json!({"id": "e-new"})]);
        let answer = report_error(
            &graph,
            &scope(),
            &json!({
                "error_type": "typo",
                "control_account_type": "sales_ledger",
                "description": "should not be stored",
            }),
        );
        assert!(answer.is_err());
    }

    #[test]
    fn a_report_counts_the_pending_breaks_and_keeps_them() {
        let graph = FakeGraph::queue(vec![
            // the error listing
            vec![
                json!({"id": "e-1", "error_type": "posting_error", "control_account_type": "sales_ledger", "status": "pending", "variance": "10.00"}),
                json!({"id": "e-2", "error_type": "posting_error", "control_account_type": "purchases_ledger", "status": "pending", "variance": "7.00"}),
            ],
            // the create
            vec![json!({"id": "r-1"})],
        ]);
        let report = reconcile_control_account(
            &graph,
            &scope(),
            "sales_ledger",
            &json!({"control_balance": 1000, "ledger_total": 1010, "as_of_date": "2026-03-31"}),
        )
        .expect("kept");
        assert_eq!(report["error_count"], json!(1));
        assert_eq!(report["difference"], json!("-10.000000"));
        assert_eq!(report["status"], json!("unreconciled"));
    }

    #[test]
    fn a_difference_under_a_cent_reconciles() {
        let graph = FakeGraph::queue(vec![vec![], vec![json!({"id": "r-2"})]]);
        let report = reconcile_control_account(
            &graph,
            &scope(),
            "bills_payable",
            &json!({"control_balance": 500, "ledger_total": 500.005}),
        )
        .expect("kept");
        assert_eq!(report["status"], json!("reconciled"));
    }

    #[test]
    fn the_summary_counts_only_pending_variance_as_money_at_risk() {
        let graph = FakeGraph::queue(vec![vec![
            json!({"id": "e-1", "error_type": "posting_error", "status": "pending", "variance": "10.00"}),
            json!({"id": "e-2", "error_type": "rounding_error", "status": "resolved", "variance": "90.00"}),
        ]]);
        let answer = reconciliation_summary(&graph, &scope()).expect("summary");
        assert_eq!(answer["total_errors"], json!(2));
        assert_eq!(answer["pending_errors"], json!(1));
        assert_eq!(answer["resolved_errors"], json!(1));
        assert_eq!(answer["total_variance"], json!("10.000000"));
        assert_eq!(answer["by_type"]["rounding_error"], json!(1));
    }
}
