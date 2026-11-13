#![forbid(unsafe_code)]
//! purchases-ledger-control-service: creditor transactions, the balance each creditor runs, and the control account.
//!
//! The ledger is the transactions and nothing else. A creditor's balance is replayed from them in creation
//! order - a purchase invoice adds, a debit note, payment, refund and discount each take away - so the balance
//! is never a stored figure that can drift from the entries behind it.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the sign a transaction type carries is decided in one place, so the stamp on a new entry and the replay
//!   can never disagree about which way a type moves;
//! * a type that neither adds nor takes away (a discount, per the shipped sign table, and anything unknown) is
//!   accepted and moves nothing;
//! * the control account reports invoices, debit notes and payments, and its closing figure is invoices less
//!   debit notes less payments - which is also what the per-creditor replay reaches;
//! * the reconciliation counts anything under a cent as reconciled;
//! * an as-at date keeps the transactions up to that day, read as calendar days rather than by clock time.

use serde_json::{json, Value};

use vimbai_common::dates::day_of;
use vimbai_common::ledger::replayed_balances;
use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, CREDITOR_TRANSACTION};

const PURCHASE_INVOICE: &str = "purchase_invoice";
const TAKING_AWAY: [&str; 3] = ["debit_note", "payment", "refund"];

/// Which way a transaction type moves a creditor balance, in one place.
///
/// A discount is in neither list: the shipped service accepted it as a type but left it out of both sign
/// sets, so a discount entry is recorded and moves nothing. Kept as shipped, pinned by a test.
fn movement(kind: &str) -> i8 {
    if kind == PURCHASE_INVOICE {
        1
    } else if TAKING_AWAY.contains(&kind) {
        -1
    } else {
        0
    }
}

fn transactions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Vec<Value>, StoreError> {
    Records::new(graph, CREDITOR_TRANSACTION).list(scope, 2000)
}

/// Each creditor's balance, replayed from the ledger by the one shared walk.
fn replayed(rows: &[Value]) -> Vec<(String, Money)> {
    replayed_balances(rows, "creditor_id", movement)
}

fn balance_of(balances: &[(String, Money)], creditor_id: &str) -> Money {
    balances
        .iter()
        .find(|(creditor, _)| creditor == creditor_id)
        .map(|(_, balance)| balance.clone())
        .unwrap_or_else(Money::zero)
}

/// `POST /transactions`.
pub fn record_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let creditor = text(payload, "creditor_id");
    if creditor.is_empty() {
        return Err(refused("creditor_id is required"));
    }
    if text(payload, "creditor_name").is_empty() {
        return Err(refused("creditor_name is required"));
    }
    let moved = amount_or_zero(payload, "amount")?;
    let running = balance_of(&replayed(&transactions(graph, scope)?), &creditor);
    let direction = movement(&text(payload, "transaction_type"));
    let after = if direction >= 0 {
        running.plus(&moved)
    } else {
        running.minus(&moved)
    };

    let mut body = payload.clone();
    match body.as_object_mut() {
        Some(fields) => {
            fields.remove("id");
            fields.entry("date").or_insert(json!(""));
            // the balance is the ledger's to stamp, never the caller's
            fields.insert("balance".into(), wire(&after));
        }
        None => return Err(refused("a transaction must be an object")),
    }
    remember(graph, scope, CREDITOR_TRANSACTION, body)
}

/// `GET /control-account/summary`.
pub fn purchases_control_summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let as_at = query
        .get("as_of_date")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let cutoff = as_at.as_deref().and_then(day_of);

    let mut invoices = Money::zero();
    let mut debit_notes = Money::zero();
    let mut payments = Money::zero();
    let mut counted = 0usize;
    let mut creditors: Vec<String> = Vec::new();

    for row in transactions(graph, scope)?.iter() {
        if let Some(cutoff) = cutoff {
            match day_of(field(row, "date")) {
                Some(day) if day <= cutoff => {}
                _ => continue,
            }
        }
        let moved = stored_amount(row, "amount").unwrap_or_else(|_| Money::zero());
        match field(row, "transaction_type") {
            PURCHASE_INVOICE => invoices = invoices.plus(&moved),
            "debit_note" => debit_notes = debit_notes.plus(&moved),
            "payment" => payments = payments.plus(&moved),
            _ => {}
        }
        counted += 1;
        let creditor = field(row, "creditor_id").to_owned();
        if !creditor.is_empty() && !creditors.contains(&creditor) {
            creditors.push(creditor);
        }
    }

    let closing = invoices.minus(&debit_notes).minus(&payments);

    Ok(json!({
        "as_of_date": as_at.unwrap_or_default(),
        "total_invoices": wire(&invoices),
        "total_debit_notes": wire(&debit_notes),
        "total_payments": wire(&payments),
        "closing_balance": wire(&closing),
        "transaction_count": counted,
        "creditor_count": creditors.len(),
    }))
}

/// `GET /creditors`.
pub fn list_creditors(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let balances = replayed(&transactions(graph, scope)?);
    let total = Money::sum(balances.iter().map(|(_, balance)| balance));
    Ok(json!({
        "creditors": balances
            .iter()
            .map(|(creditor, balance)| json!({"creditor_id": creditor, "balance": wire(balance)}))
            .collect::<Vec<_>>(),
        "total_balance": wire(&total),
    }))
}

/// `POST /reconcile`: the control account against the sum of the creditor balances.
pub fn purchases_reconcile(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let ledger = Money::sum(
        replayed(&transactions(graph, scope)?)
            .iter()
            .map(|(_, balance)| balance),
    );
    let control = purchases_control_summary(graph, scope, &json!({}))?;
    let control =
        Money::parse(&text(&control, "closing_balance")).unwrap_or_else(|_| Money::zero());
    let difference = control.minus(&ledger).abs();
    let reconciled = difference.to_f64_lossy() < 0.01;

    Ok(json!({
        "control_account_balance": wire(&control),
        "purchases_ledger_total": wire(&ledger),
        "difference": wire(&difference),
        "reconciled": reconciled,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn ledger() -> Vec<Value> {
        vec![
            json!({"id": "t-1", "creditor_id": "c-1", "transaction_type": "purchase_invoice", "amount": "1000.000000", "date": "2026-01-01"}),
            json!({"id": "t-2", "creditor_id": "c-1", "transaction_type": "payment", "amount": "400.000000", "date": "2026-01-10"}),
            json!({"id": "t-3", "creditor_id": "c-2", "transaction_type": "purchase_invoice", "amount": "250.500000", "date": "2026-01-20"}),
            json!({"id": "t-4", "creditor_id": "c-1", "transaction_type": "debit_note", "amount": "100.000000", "date": "2026-02-01"}),
        ]
    }

    #[test]
    fn a_creditors_balance_is_the_replay_of_its_own_transactions() {
        let graph = FakeGraph::returning(ledger());
        let answer = list_creditors(&graph, &scope()).expect("listed");
        let creditors = answer["creditors"].as_array().unwrap();
        assert_eq!(creditors.len(), 2);
        assert_eq!(creditors[0]["creditor_id"], json!("c-1"));
        assert_eq!(creditors[0]["balance"], json!("500.000000"));
        assert_eq!(answer["total_balance"], json!("750.500000"));
    }

    #[test]
    fn the_control_account_is_the_movements_and_its_closing_figure_agrees_with_the_replay() {
        let graph = FakeGraph::returning(ledger());
        let answer = purchases_control_summary(&graph, &scope(), &json!({})).expect("summarised");
        assert_eq!(answer["total_invoices"], json!("1250.500000"));
        assert_eq!(answer["total_debit_notes"], json!("100.000000"));
        assert_eq!(answer["total_payments"], json!("400.000000"));
        assert_eq!(answer["closing_balance"], json!("750.500000"));
        assert_eq!(answer["transaction_count"], json!(4));
        assert_eq!(answer["creditor_count"], json!(2));
    }

    #[test]
    fn an_as_at_date_keeps_only_the_transactions_up_to_that_day() {
        let graph = FakeGraph::returning(ledger());
        let answer =
            purchases_control_summary(&graph, &scope(), &json!({"as_of_date": "2026-01-20"}))
                .expect("summarised");
        assert_eq!(answer["transaction_count"], json!(3));
        assert_eq!(answer["closing_balance"], json!("850.500000"));
        assert_eq!(answer["as_of_date"], json!("2026-01-20"));
    }

    #[test]
    fn reconciling_compares_the_two_and_counts_a_cent_as_agreed() {
        let graph = FakeGraph::returning(ledger());
        let answer = purchases_reconcile(&graph, &scope()).expect("reconciled");
        assert_eq!(answer["control_account_balance"], json!("750.500000"));
        assert_eq!(answer["purchases_ledger_total"], json!("750.500000"));
        assert_eq!(answer["difference"], json!("0.000000"));
        assert_eq!(answer["reconciled"], json!(true));
    }

    #[test]
    fn a_discount_is_recorded_but_moves_nothing() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "t-1", "creditor_id": "c-1", "transaction_type": "purchase_invoice", "amount": "1000.000000", "date": "2026-01-01"}),
            json!({"id": "t-2", "creditor_id": "c-1", "transaction_type": "discount", "amount": "100.000000", "date": "2026-01-05"}),
        ]);
        let answer = list_creditors(&graph, &scope()).expect("listed");
        assert_eq!(answer["creditors"][0]["balance"], json!("1000.000000"));
        let control = purchases_control_summary(&graph, &scope(), &json!({})).expect("summarised");
        assert_eq!(control["transaction_count"], json!(2));
    }

    #[test]
    fn a_transaction_is_stamped_with_the_balance_it_leaves_behind() {
        let graph = FakeGraph::queue(vec![
            ledger(),
            vec![json!({"id": "t-5", "balance": "700.500000"})],
        ]);
        record_transaction(
            &graph,
            &scope(),
            &json!({
                "creditor_id": "c-1",
                "creditor_name": "Harare Wholesale",
                "transaction_type": "payment",
                "amount": 300,
                "date": "2026-03-01",
            }),
        )
        .expect("kept");
        let (_, params) = graph.last();
        // 1000 invoiced, 400 paid, 100 credited back, then this payment of 300
        assert_eq!(params["props"]["balance"], json!("200.000000"));
        assert_eq!(params["props"]["creditor_name"], json!("Harare Wholesale"));
    }

    #[test]
    fn a_purchase_invoice_adds_to_what_is_owed() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "t-1", "creditor_id": "c-1", "transaction_type": "purchase_invoice", "amount": "1000.000000", "date": "2026-01-01"}),
            ],
            vec![json!({"id": "t-2"})],
        ]);
        let answer = record_transaction(
            &graph,
            &scope(),
            &json!({
                "creditor_id": "c-1",
                "creditor_name": "Harare Wholesale",
                "transaction_type": "purchase_invoice",
                "amount": 250,
            }),
        )
        .expect("kept");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["balance"], json!("1250.000000"));
        // a claim with no date keeps the field the store fills rather than inventing one
        assert_eq!(params["props"]["date"], json!(""));
        assert_eq!(answer["id"], json!("t-2"));
    }

    #[test]
    fn a_transaction_without_a_creditor_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let refused = record_transaction(
            &graph,
            &scope(),
            &json!({"transaction_type": "payment", "amount": 100}),
        );
        assert!(refused.is_err());
    }
}
