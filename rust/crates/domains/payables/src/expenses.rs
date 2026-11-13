#![forbid(unsafe_code)]
//! expense-tracking-service: an employee's expense claims, their approval or rejection, and the summary.
//!
//! An expense is one record with its own status rather than a ledger, so it is created once and then only
//! moved between states. Behaviours kept as shipped, each pinned by a test:
//!
//! * an expense arrives pending whatever the caller says: the status, the approver and the rejection reason
//!   belong to the service, not to the request;
//! * an amount that is not above nil is refused, because an expense of nothing is not an expense, and a
//!   category outside the accepted kinds is refused by name rather than stored as text;
//! * approval records who approved and rejection records why, and each answers only the three fields the
//!   shipped route answered rather than the whole record;
//! * the listing is newest first, narrowed by category and status, and a page larger than the service's own
//!   largest is refused rather than quietly filled;
//! * the summary measures that same listing, so it counts at most the listing's own page of claims - the
//!   shipped shape, pinned rather than smoothed over.

use serde_json::{json, Value};

use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, refused, text};
use vimbai_store::{BookScope, Graph, Records, StoreError, EXPENSE};

const PENDING: &str = "pending";
const APPROVED: &str = "approved";
const REJECTED: &str = "rejected";
/// The listing's own default page, and the largest page it will answer.
const DEFAULT_PAGE: usize = 100;
const LARGEST_PAGE: usize = 1000;
/// The categories the shipped service accepted, and the four states a claim can hold.
const CATEGORIES: [&str; 9] = [
    "travel",
    "meals",
    "office",
    "utilities",
    "software",
    "equipment",
    "professional",
    "marketing",
    "other",
];

fn store<'a, G: Graph + ?Sized>(graph: &'a G) -> Records<'a, G> {
    Records::new(graph, EXPENSE)
}

/// The page the caller asked for, refused above the largest the service answers.
fn page_of(payload: &Value) -> Result<usize, StoreError> {
    let asked = match payload.get("limit") {
        Some(Value::Number(number)) => number.as_u64().unwrap_or(DEFAULT_PAGE as u64),
        Some(Value::String(raw)) => raw.trim().parse::<u64>().unwrap_or(DEFAULT_PAGE as u64),
        _ => DEFAULT_PAGE as u64,
    };
    if asked > LARGEST_PAGE as u64 {
        return Err(refused("limit may be at most 1000"));
    }
    Ok(asked.max(1) as usize)
}

/// `POST /expenses`.
pub fn claim_expense(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    for required in ["company_id", "employee_id"] {
        if text(payload, required).is_empty() {
            return Err(refused(&format!("{required} is required")));
        }
    }
    let category = text(payload, "category");
    if !CATEGORIES.contains(&category.as_str()) {
        return Err(refused(&format!(
            "category {category} is not one of the accepted kinds"
        )));
    }
    let amount = amount_or_zero(payload, "amount")?;
    if amount.to_f64_lossy() <= 0.0 {
        return Err(refused("amount must be greater than zero"));
    }

    let mut body = payload.clone();
    match body.as_object_mut() {
        Some(fields) => {
            fields.remove("id");
            // the state of a claim is the service's to set, whatever the caller sent
            fields.insert("status".into(), json!(PENDING));
            fields.insert("approved_by".into(), json!(""));
            fields.insert("rejection_reason".into(), json!(""));
            fields.insert("amount".into(), wire(&amount));
            fields.entry("currency").or_insert_with(|| json!("USD"));
        }
        None => return Err(refused("an expense must be an object")),
    }
    vimbai_store::write::remember(graph, scope, EXPENSE, body)
}

/// The caller's claims for a company, newest first, narrowed by category and status.
fn claims(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Vec<Value>, StoreError> {
    let company = text(payload, "company_id");
    if company.is_empty() {
        return Err(refused("company_id is required"));
    }
    let category = text(payload, "category");
    let status = text(payload, "status");
    let page = page_of(payload)?;
    let mut kept: Vec<Value> = store(graph)
        .list(scope, LARGEST_PAGE)?
        .into_iter()
        .filter(|row| field(row, "company_id") == company)
        .filter(|row| category.is_empty() || field(row, "category") == category)
        .filter(|row| status.is_empty() || field(row, "status") == status)
        .collect();
    kept.truncate(page);
    Ok(kept)
}

/// `GET /expenses/{company_id}`.
pub fn list_expense_claims(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let mut body = payload.clone();
    match body.as_object_mut() {
        Some(fields) => {
            fields.insert("company_id".into(), json!(company_id));
        }
        None => body = json!({"company_id": company_id}),
    }
    let kept = claims(graph, scope, &body)?;
    Ok(json!({
        "company_id": company_id,
        "expenses": kept,
        "total": kept.len(),
    }))
}

fn moved_to(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    expense_id: &str,
    patch: Value,
) -> Result<Value, StoreError> {
    store(graph).update(scope, expense_id, &patch)
}

/// `PUT /expenses/{expense_id}/approve`.
pub fn approve_expense(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    expense_id: &str,
    approver: &str,
) -> Result<Value, StoreError> {
    let approved = moved_to(
        graph,
        scope,
        expense_id,
        json!({"status": APPROVED, "approved_by": approver}),
    )?;
    Ok(json!({
        "id": field(&approved, "id"),
        "status": field(&approved, "status"),
        "approved_by": field(&approved, "approved_by"),
    }))
}

/// `PUT /expenses/{expense_id}/reject`.
pub fn reject_expense(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    expense_id: &str,
    reason: &str,
) -> Result<Value, StoreError> {
    let rejected = moved_to(
        graph,
        scope,
        expense_id,
        json!({"status": REJECTED, "rejection_reason": reason}),
    )?;
    Ok(json!({
        "id": field(&rejected, "id"),
        "status": field(&rejected, "status"),
        "rejection_reason": field(&rejected, "rejection_reason"),
    }))
}

/// `GET /summary/{company_id}`: the same listing counted by category and by state.
pub fn expense_summary(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    company_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let mut body = payload.clone();
    match body.as_object_mut() {
        Some(fields) => {
            fields.insert("company_id".into(), json!(company_id));
        }
        None => body = json!({"company_id": company_id}),
    }
    let kept = claims(graph, scope, &body)?;

    let mut categories: Vec<(String, Money)> = Vec::new();
    let mut states: Vec<(String, usize)> = Vec::new();
    let mut total = Money::zero();
    for row in &kept {
        let moved = stored_amount(row, "amount").unwrap_or_else(|_| Money::zero());
        total = total.plus(&moved);
        let category = field(row, "category").to_owned();
        match categories.iter().position(|(seen, _)| *seen == category) {
            Some(at) => categories[at].1 = categories[at].1.plus(&moved),
            None => categories.push((category, moved)),
        }
        let status = field(row, "status").to_owned();
        match states.iter().position(|(seen, _)| *seen == status) {
            Some(at) => states[at].1 += 1,
            None => states.push((status, 1)),
        }
    }

    Ok(json!({
        "company_id": company_id,
        "total_expenses": kept.len(),
        "total_amount": wire(&total),
        "by_category": categories
            .iter()
            .map(|(category, amount)| (category.clone(), wire(amount)))
            .collect::<serde_json::Map<String, Value>>(),
        "by_status": states
            .iter()
            .map(|(status, count)| (status.clone(), json!(count)))
            .collect::<serde_json::Map<String, Value>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn claim(id: &str, category: &str, status: &str, amount: &str) -> Value {
        json!({
            "id": id,
            "company_id": "co-1",
            "employee_id": "emp-1",
            "category": category,
            "amount": amount,
            "status": status,
            "created_at": "2026-03-01T00:00:00",
        })
    }

    #[test]
    fn an_expense_arrives_pending_whatever_the_caller_said() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "e-1"})]]);
        let answer = claim_expense(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1",
                "employee_id": "emp-1",
                "category": "travel",
                "amount": 240.5,
                "status": "approved",
                "approved_by": "someone else",
            }),
        )
        .expect("kept");
        let (_, params) = graph.last();
        assert_eq!(params["props"]["status"], json!("pending"));
        assert_eq!(params["props"]["approved_by"], json!(""));
        assert_eq!(params["props"]["amount"], json!("240.500000"));
        assert_eq!(params["props"]["currency"], json!("USD"));
        assert_eq!(answer["id"], json!("e-1"));
    }

    #[test]
    fn an_expense_of_nothing_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let nothing = claim_expense(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "employee_id": "emp-1", "category": "meals", "amount": 0}),
        );
        assert!(nothing.is_err());
        let negative = claim_expense(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "employee_id": "emp-1", "category": "meals", "amount": -5}),
        );
        assert!(negative.is_err());
    }

    #[test]
    fn a_category_outside_the_accepted_kinds_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let refused = claim_expense(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "employee_id": "emp-1", "category": "bribes", "amount": 10}),
        );
        assert!(refused.is_err());
    }

    #[test]
    fn the_listing_is_narrowed_by_category_and_state() {
        let graph = FakeGraph::returning(vec![
            claim("e-1", "travel", "approved", "240.500000"),
            claim("e-2", "meals", "pending", "40.000000"),
            claim("e-3", "travel", "pending", "100.000000"),
            json!({"id": "e-4", "company_id": "co-2", "category": "travel", "amount": "999.000000", "status": "pending"}),
        ]);
        let answer = list_expense_claims(&graph, &scope(), "co-1", &json!({"category": "travel"}))
            .expect("listed");
        assert_eq!(answer["total"], json!(2));
        assert_eq!(answer["expenses"][0]["id"], json!("e-1"));

        let pending = list_expense_claims(
            &graph,
            &scope(),
            "co-1",
            &json!({"category": "travel", "status": "pending"}),
        )
        .expect("listed");
        assert_eq!(pending["total"], json!(1));
        assert_eq!(pending["expenses"][0]["id"], json!("e-3"));
    }

    #[test]
    fn a_page_larger_than_the_largest_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let refused = list_expense_claims(&graph, &scope(), "co-1", &json!({"limit": 5000}));
        assert!(refused.is_err());
        assert!(list_expense_claims(&graph, &scope(), "co-1", &json!({"limit": 1000})).is_ok());
    }

    #[test]
    fn approving_records_who_and_answers_three_fields() {
        let graph = FakeGraph::queue(vec![vec![json!({
            "id": "e-1", "company_id": "co-1", "category": "travel", "amount": "240.500000",
            "status": "approved", "approved_by": "manager-1"
        })]]);
        let answer = approve_expense(&graph, &scope(), "e-1", "manager-1").expect("approved");
        assert_eq!(answer["id"], json!("e-1"));
        assert_eq!(answer["status"], json!("approved"));
        assert_eq!(answer["approved_by"], json!("manager-1"));
        let (statement, params) = graph.last();
        assert!(statement.contains("SET"), "{statement}");
        assert_eq!(params["patch"]["status"], json!("approved"));
        assert_eq!(params["patch"]["approved_by"], json!("manager-1"));
    }

    #[test]
    fn rejecting_records_why() {
        let graph = FakeGraph::queue(vec![vec![json!({
            "id": "e-1", "company_id": "co-1", "category": "meals", "amount": "40.000000",
            "status": "rejected", "rejection_reason": "no receipt"
        })]]);
        let answer = reject_expense(&graph, &scope(), "e-1", "no receipt").expect("rejected");
        assert_eq!(answer["status"], json!("rejected"));
        assert_eq!(answer["rejection_reason"], json!("no receipt"));
        let (_, params) = graph.last();
        assert_eq!(params["patch"]["rejection_reason"], json!("no receipt"));
    }

    #[test]
    fn a_claim_that_is_not_the_callers_answers_as_a_missing_one() {
        let graph = FakeGraph::returning(vec![]);
        assert!(approve_expense(&graph, &scope(), "e-9", "manager-1").is_err());
    }

    #[test]
    fn the_summary_adds_by_category_and_counts_by_state() {
        let graph = FakeGraph::returning(vec![
            claim("e-1", "travel", "approved", "240.500000"),
            claim("e-2", "travel", "pending", "100.000000"),
            claim("e-3", "meals", "pending", "40.000000"),
        ]);
        let answer = expense_summary(&graph, &scope(), "co-1", &json!({})).expect("summarised");
        assert_eq!(answer["total_expenses"], json!(3));
        assert_eq!(answer["total_amount"], json!("380.500000"));
        assert_eq!(answer["by_category"]["travel"], json!("340.500000"));
        assert_eq!(answer["by_category"]["meals"], json!("40.000000"));
        assert_eq!(answer["by_status"]["pending"], json!(2));
        assert_eq!(answer["by_status"]["approved"], json!(1));
    }

    #[test]
    fn the_summary_counts_the_listings_own_page_rather_than_everything() {
        // the shipped summary measured the listing, whose page is a hundred: the hundred and first claim is not
        // in the page, so it is not in the summary either. Kept as shipped.
        let graph = FakeGraph::returning(vec![claim("e-1", "travel", "pending", "10.000000")]);
        let answer =
            expense_summary(&graph, &scope(), "co-1", &json!({"limit": 1})).expect("summarised");
        assert_eq!(answer["total_expenses"], json!(1));
        let (_, params) = graph.last();
        assert_eq!(params["limit"], json!(1000));
    }
}
