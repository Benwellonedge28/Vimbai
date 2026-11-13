#![forbid(unsafe_code)]
//! sales-ledger-control-service: debtor transactions, the balance each debtor runs, and the control account.
//!
//! The ledger is the transactions and nothing else. A debtor's balance is replayed from them in creation
//! order - an invoice adds, a credit note, payment, refund and bad debt each take away - so the balance is
//! never a stored figure that can drift from the entries behind it.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a transaction type that neither adds nor takes away is accepted and moves nothing;
//! * the control account is the four movement totals, and its closing figure is invoices less credit notes,
//!   payments and bad debts - which is the same number the per-debtor replay reaches, so reconciling the two
//!   is a comparison rather than a calculation;
//! * the reconciliation counts anything under a cent as reconciled;
//! * an as-at date keeps the transactions up to that day, read as calendar days rather than by clock time;
//! * a debtor balance reports the last ten transactions of that debtor, not the last ten in the book.

use serde_json::{json, Value};

use vimbai_common::dates::day_of;
use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, DEBTOR_TRANSACTION};

const INVOICE: &str = "invoice";
const TAKING_AWAY: [&str; 4] = ["credit_note", "payment", "refund", "bad_debt"];
const LAST_SHOWN: usize = 10;

/// What each balance-adding or balance-taking transaction does, in one place.
fn movement(kind: &str) -> i8 {
    if kind == INVOICE {
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
    Records::new(graph, DEBTOR_TRANSACTION).list(scope, 2000)
}

/// Each debtor's balance, replayed in creation order.
fn replayed(rows: &[Value]) -> Vec<(String, Money)> {
    let mut order: Vec<String> = Vec::new();
    let mut balances: Vec<Money> = Vec::new();
    for row in rows {
        let debtor = field(row, "debtor_id").to_owned();
        let moved = stored_amount(row, "amount").unwrap_or_else(|_| Money::zero());
        let direction = movement(field(row, "transaction_type"));
        match order.iter().position(|seen| *seen == debtor) {
            Some(at) => {
                let next = if direction >= 0 {
                    balances[at].plus(&moved)
                } else {
                    balances[at].minus(&moved)
                };
                balances[at] = next;
            }
            None if direction != 0 => {
                order.push(debtor);
                balances.push(if direction > 0 { moved } else { moved.negate() });
            }
            None => {}
        }
    }
    order.into_iter().zip(balances).collect()
}

fn balance_of(balances: &[(String, Money)], debtor_id: &str) -> Money {
    balances
        .iter()
        .find(|(debtor, _)| debtor == debtor_id)
        .map(|(_, balance)| balance.clone())
        .unwrap_or_else(Money::zero)
}

/// `POST /transactions`.
pub fn add_debtor_transaction(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let debtor = text(payload, "debtor_id");
    if debtor.is_empty() {
        return Err(vimbai_store::read::refused("debtor_id is required"));
    }
    let moved = amount_or_zero(payload, "amount")?;
    let before = replayed(&transactions(graph, scope)?);
    let running = balance_of(&before, &debtor);
    let direction = movement(&text(payload, "transaction_type"));
    let after = if direction >= 0 {
        running.plus(&moved)
    } else {
        running.minus(&moved)
    };

    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("id");
        fields.entry("date").or_insert(json!(""));
        // the balance is the ledger's to stamp, never the caller's
        fields.insert("balance".into(), wire(&after));
    } else {
        return Err(vimbai_store::read::refused(
            "a transaction must be an object",
        ));
    }
    remember(graph, scope, DEBTOR_TRANSACTION, body)
}

/// `GET /control-account/summary`.
pub fn control_summary(
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
    let mut credit_notes = Money::zero();
    let mut payments = Money::zero();
    let mut bad_debts = Money::zero();
    let mut counted = 0usize;
    let mut debtors: Vec<String> = Vec::new();

    for row in transactions(graph, scope)?.iter() {
        if let Some(cutoff) = cutoff {
            match day_of(field(row, "date")) {
                Some(day) if day <= cutoff => {}
                _ => continue,
            }
        }
        let moved = stored_amount(row, "amount").unwrap_or_else(|_| Money::zero());
        match field(row, "transaction_type") {
            INVOICE => invoices = invoices.plus(&moved),
            "credit_note" => credit_notes = credit_notes.plus(&moved),
            "payment" => payments = payments.plus(&moved),
            "bad_debt" => bad_debts = bad_debts.plus(&moved),
            _ => {}
        }
        counted += 1;
        let debtor = field(row, "debtor_id").to_owned();
        if !debtor.is_empty() && !debtors.contains(&debtor) {
            debtors.push(debtor);
        }
    }

    let closing = invoices
        .minus(&credit_notes)
        .minus(&payments)
        .minus(&bad_debts);

    Ok(json!({
        "as_of_date": as_at.unwrap_or_default(),
        "total_invoices": wire(&invoices),
        "total_credit_notes": wire(&credit_notes),
        "total_payments": wire(&payments),
        "total_bad_debts": wire(&bad_debts),
        "closing_balance": wire(&closing),
        "transaction_count": counted,
        "debtor_count": debtors.len(),
    }))
}

/// `GET /debtors/{debtor_id}/balance`.
pub fn debtor_balance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debtor_id: &str,
) -> Result<Value, StoreError> {
    let rows = transactions(graph, scope)?;
    let balance = balance_of(&replayed(&rows), debtor_id);
    let held: Vec<&Value> = rows
        .iter()
        .filter(|row| field(row, "debtor_id") == debtor_id)
        .collect();
    let recent: Vec<Value> = held
        .iter()
        .rev()
        .take(LAST_SHOWN)
        .rev()
        .map(|row| (*row).clone())
        .collect();

    Ok(json!({
        "debtor_id": debtor_id,
        "balance": wire(&balance),
        "transaction_count": held.len(),
        "transactions": recent,
    }))
}

/// `GET /debtors`.
pub fn list_debtors(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let balances = replayed(&transactions(graph, scope)?);
    let total = Money::sum(balances.iter().map(|(_, balance)| balance));
    Ok(json!({
        "debtors": balances
            .iter()
            .map(|(debtor, balance)| json!({"debtor_id": debtor, "balance": wire(balance)}))
            .collect::<Vec<_>>(),
        "total_balance": wire(&total),
    }))
}

/// `POST /reconcile`: the control account against the sum of the debtor balances.
pub fn reconcile(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let ledger = Money::sum(
        replayed(&transactions(graph, scope)?)
            .iter()
            .map(|(_, b)| b),
    );
    let control = control_summary(graph, scope, &json!({}))?;
    let control =
        Money::parse(&text(&control, "closing_balance")).unwrap_or_else(|_| Money::zero());
    let difference = control.minus(&ledger).abs();
    let reconciled = difference.to_f64_lossy() < 0.01;

    Ok(json!({
        "control_account_balance": wire(&control),
        "sales_ledger_total": wire(&ledger),
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
            json!({"id": "t-1", "debtor_id": "d-1", "transaction_type": "invoice", "amount": "1000.000000", "date": "2026-01-01"}),
            json!({"id": "t-2", "debtor_id": "d-1", "transaction_type": "payment", "amount": "400.000000", "date": "2026-01-10"}),
            json!({"id": "t-3", "debtor_id": "d-2", "transaction_type": "invoice", "amount": "250.500000", "date": "2026-01-20"}),
            json!({"id": "t-4", "debtor_id": "d-1", "transaction_type": "credit_note", "amount": "100.000000", "date": "2026-02-01"}),
        ]
    }

    #[test]
    fn a_debtors_balance_is_the_replay_of_its_own_transactions() {
        let graph = FakeGraph::returning(ledger());
        let answer = debtor_balance(&graph, &scope(), "d-1").expect("balanced");
        assert_eq!(answer["balance"], json!("500.000000"));
        assert_eq!(answer["transaction_count"], json!(3));
    }

    #[test]
    fn the_control_account_is_the_movements_and_its_closing_figure_agrees_with_the_replay() {
        let graph = FakeGraph::returning(ledger());
        let answer = control_summary(&graph, &scope(), &json!({})).expect("summarised");
        assert_eq!(answer["total_invoices"], json!("1250.500000"));
        assert_eq!(answer["total_payments"], json!("400.000000"));
        assert_eq!(answer["closing_balance"], json!("750.500000"));
        assert_eq!(answer["debtor_count"], json!(2));
    }

    #[test]
    fn an_as_at_date_keeps_only_the_transactions_up_to_that_day() {
        let graph = FakeGraph::returning(ledger());
        let answer = control_summary(&graph, &scope(), &json!({"as_of_date": "2026-01-20"}))
            .expect("summarised");
        assert_eq!(answer["transaction_count"], json!(3));
        assert_eq!(answer["closing_balance"], json!("850.500000"));
    }

    #[test]
    fn reconciling_compares_the_two_and_counts_a_cent_as_agreed() {
        let graph = FakeGraph::returning(ledger());
        let answer = reconcile(&graph, &scope()).expect("reconciled");
        assert_eq!(answer["difference"], json!("0.000000"));
        assert_eq!(answer["reconciled"], json!(true));
    }

    #[test]
    fn a_type_that_neither_adds_nor_takes_away_moves_nothing() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "t-1", "debtor_id": "d-1", "transaction_type": "memo", "amount": "500.000000",
            "date": "2026-01-01"
        })]);
        let answer = list_debtors(&graph, &scope()).expect("listed");
        assert_eq!(answer["total_balance"], json!("0.000000"));
        assert_eq!(answer["debtors"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn a_transaction_is_stamped_with_the_balance_it_leaves_behind() {
        let graph = FakeGraph::queue(vec![
            ledger(),
            vec![json!({"id": "t-5", "balance": "700.500000"})],
        ]);
        let answer = add_debtor_transaction(
            &graph,
            &scope(),
            &json!({"debtor_id": "d-1", "transaction_type": "payment", "amount": 300, "date": "2026-03-01"}),
        )
        .expect("kept");
        assert_eq!(answer["balance"], json!("700.500000"));
    }
}
