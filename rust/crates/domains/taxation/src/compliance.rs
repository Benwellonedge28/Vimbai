#![forbid(unsafe_code)]
//! tax-compliance-service: the obligations a company owes, filing them, and the compliance summary.
//!
//! An obligation is a due date and an amount that has not been paid yet, so nothing here is arithmetic on a
//! rate; what matters is what counts as compliant and which deadline is coming.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * an obligation is created `pending` whatever the caller says - the status is the service's to move;
//! * the list is ordered by due date, and the status filter is applied to the answer rather than in the query,
//!   so an obligation whose status was changed is still found by the date it is due;
//! * filing takes the amount stated, and when the caller states nothing it files the obligation's own amount
//!   rather than zero - a filing that names no figure settles what was due;
//! * filing an obligation that belongs to another company answers exactly as a nonexistent one does, so the
//!   register cannot be used to discover another tenant's obligations;
//! * a company with no obligations scores 100, because nothing is late;
//! * a deadline counts as upcoming when it falls due within thirty days of the day asked about, and an already
//!   overdue one is included with a negative count, since it is the most urgent thing on the list;
//! * a due date that cannot be read is left out of the upcoming list rather than counted as zero days away.

use serde_json::{json, Value};

use vimbai_common::dates::days_between;
use vimbai_store::amounts::{stored_amount, wire};
use vimbai_store::read::{field, money_or_nothing, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, TAX_OBLIGATION};

const LISTED: usize = 2000;
const PENDING: &str = "pending";
const FILED: &str = "filed";
const COMPLIANT: &str = "compliant";
const OVERDUE: &str = "overdue";

fn rows<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    company_id: &str,
) -> Result<Vec<Value>, StoreError> {
    let mut found: Vec<Value> = Records::new(graph, TAX_OBLIGATION)
        .list(scope, LISTED)?
        .into_iter()
        .filter(|row| field(row, "company_id") == company_id)
        .collect();
    found.sort_by(|left, right| field(left, "due_date").cmp(field(right, "due_date")));
    Ok(found)
}

fn company(payload: &Value) -> Result<String, StoreError> {
    let company_id = text(payload, "company_id");
    if company_id.is_empty() {
        return Err(StoreError::Invalid("company_id is required".into()));
    }
    Ok(company_id)
}

pub fn create_obligation<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = company(payload)?;
    let obligation_type = text(payload, "obligation_type");
    if obligation_type.is_empty() {
        return Err(StoreError::Invalid("obligation_type is required".into()));
    }
    let due_date = text(payload, "due_date");
    if due_date.is_empty() {
        return Err(StoreError::Invalid("due_date is required".into()));
    }
    let amount = money_or_nothing(payload, "amount")?;
    let body = json!({
        "company_id": company_id,
        "obligation_type": obligation_type,
        "description": text(payload, "description"),
        "due_date": due_date,
        // the service owns this field: an obligation arrives owed, never already compliant
        "status": PENDING,
        "filing_frequency": vimbai_store::read::or_default(payload, "filing_frequency", "monthly"),
        "amount": wire(&amount),
    });
    remember(graph, scope, TAX_OBLIGATION, body)
}

pub fn list_obligations<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = company(payload)?;
    let status = text(payload, "status");
    let found: Vec<Value> = rows(graph, scope, &company_id)?
        .into_iter()
        .filter(|row| status.is_empty() || field(row, "status") == status)
        .collect();
    Ok(json!({
        "count": found.len(),
        "obligations": found,
    }))
}

pub fn file_obligation<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    obligation_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = company(payload)?;
    let stored = Records::new(graph, TAX_OBLIGATION).get(scope, obligation_id)?;
    if field(&stored, "company_id") != company_id {
        return Err(StoreError::NotFound);
    }
    let outstanding = stored_amount(&stored, "amount")?;
    let stated = money_or_nothing(payload, "filed_amount")?;
    // a filing that names no figure settles what was due
    let filed = if stated.is_zero() {
        outstanding
    } else {
        stated
    };
    let patch = json!({
        "status": FILED,
        "amount": wire(&filed),
    });
    Records::new(graph, TAX_OBLIGATION).update(scope, obligation_id, &patch)?;
    Ok(json!({
        "filed": true,
        "obligation_id": obligation_id,
        "amount": wire(&filed),
    }))
}

/// The compliance summary: how many of each status, the score, and the deadlines inside a month.
///
/// The day is the caller's, so the same obligations answer the same summary twice; the shipped service read
/// the clock, which made the upcoming list move under a test.
pub fn summary<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let company_id = company(payload)?;
    let today = text(payload, "today");
    let items = rows(graph, scope, &company_id)?;
    let counted = |status: &str| {
        items
            .iter()
            .filter(|row| field(row, "status") == status)
            .count()
    };
    let compliant = counted(COMPLIANT);
    let pending = counted(PENDING);
    let overdue = counted(OVERDUE);
    let filed = counted(FILED);
    let total = items.len();
    let score = if total == 0 {
        100.0
    } else {
        (compliant + filed) as f64 / total as f64 * 100.0
    };

    let mut upcoming: Vec<Value> = Vec::new();
    for row in &items {
        let status = field(row, "status");
        if status != PENDING && status != OVERDUE {
            continue;
        }
        let due = field(row, "due_date").to_owned();
        let Some(days_until) = days_between(&due, &today) else {
            // an unreadable date is neither upcoming nor zero days away
            continue;
        };
        if days_until > 30 {
            continue;
        }
        upcoming.push(json!({
            "obligation_id": field(row, "id"),
            "type": field(row, "obligation_type"),
            "due_date": due,
            "days_until": days_until,
            "amount": wire(&stored_amount(row, "amount")?),
            "status": status,
        }));
    }
    upcoming.sort_by_key(|entry| entry["days_until"].as_i64().unwrap_or(0));

    Ok(json!({
        "company_id": company_id,
        "total_obligations": total,
        "compliant": compliant,
        "pending": pending,
        "overdue": overdue,
        "filed": filed,
        "compliance_score": score,
        "upcoming_deadlines": upcoming,
        "obligations": items,
        "day": today,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn obligation(id: &str, status: &str, due: &str, amount: &str) -> Value {
        json!({
            "id": id,
            "company_id": "co-1",
            "obligation_type": "vat_return",
            "description": "VAT for the month",
            "due_date": due,
            "amount": amount,
            "status": status,
            "filing_frequency": "monthly",
        })
    }

    #[test]
    fn an_obligation_arrives_pending_whatever_the_caller_said() {
        let graph = FakeGraph::queue(vec![vec![json!({"id": "ob-1"})]]);
        let answer = create_obligation(
            &graph,
            &scope(),
            &json!({
                "company_id": "co-1",
                "obligation_type": "vat_return",
                "description": "VAT for the month",
                "due_date": "2026-03-25",
                "amount": 1200,
                "status": "compliant",
            }),
        )
        .expect("created");
        assert_eq!(field(&answer, "status"), "pending");
        assert_eq!(field(&answer, "amount"), "1200.000000");
    }

    #[test]
    fn an_obligation_without_a_type_or_a_due_date_is_refused() {
        let graph = FakeGraph::returning(vec![]);
        let no_type = create_obligation(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "due_date": "2026-03-25"}),
        );
        assert!(no_type.is_err());
        let no_due = create_obligation(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "obligation_type": "paye"}),
        );
        assert!(no_due.is_err());
    }

    #[test]
    fn the_list_is_ordered_by_due_date_and_filters_on_status() {
        let graph = FakeGraph::returning(vec![
            obligation("ob-2", "pending", "2026-04-25", "600"),
            obligation("ob-1", "compliant", "2026-03-25", "1200"),
        ]);
        let all =
            list_obligations(&graph, &scope(), &json!({"company_id": "co-1"})).expect("listed");
        assert_eq!(all["count"], json!(2));
        assert_eq!(field(&all["obligations"][0], "id"), "ob-1");
        let pending = list_obligations(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "status": "pending"}),
        )
        .expect("listed");
        assert_eq!(pending["count"], json!(1));
        assert_eq!(field(&pending["obligations"][0], "id"), "ob-2");
    }

    #[test]
    fn filing_without_a_figure_settles_what_was_due() {
        let graph = FakeGraph::queue(vec![vec![obligation(
            "ob-1",
            "pending",
            "2026-03-25",
            "1200",
        )]]);
        let answer = file_obligation(&graph, &scope(), "ob-1", &json!({"company_id": "co-1"}))
            .expect("filed");
        assert_eq!(answer["filed"], json!(true));
        assert_eq!(field(&answer, "amount"), "1200.000000");
        let (statement, params) = graph.last();
        assert!(
            statement.contains("SET"),
            "the obligation was not updated: {statement}"
        );
        assert_eq!(params["patch"]["status"], json!("filed"));
    }

    #[test]
    fn filing_a_stated_figure_uses_it() {
        let graph = FakeGraph::queue(vec![vec![obligation(
            "ob-1",
            "pending",
            "2026-03-25",
            "1200",
        )]]);
        let answer = file_obligation(
            &graph,
            &scope(),
            "ob-1",
            &json!({"company_id": "co-1", "filed_amount": 900}),
        )
        .expect("filed");
        assert_eq!(field(&answer, "amount"), "900.000000");
    }

    #[test]
    fn another_companys_obligation_answers_as_a_missing_one() {
        let graph = FakeGraph::returning(vec![obligation("ob-1", "pending", "2026-03-25", "1200")]);
        let refused = file_obligation(&graph, &scope(), "ob-1", &json!({"company_id": "co-2"}));
        assert!(matches!(refused, Err(StoreError::NotFound)));
    }

    #[test]
    fn a_company_with_nothing_due_scores_a_hundred() {
        let graph = FakeGraph::returning(vec![]);
        let answer = summary(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "today": "2026-03-01"}),
        )
        .expect("summarised");
        assert_eq!(answer["compliance_score"], json!(100.0));
        assert_eq!(answer["total_obligations"], json!(0));
    }

    #[test]
    fn the_upcoming_list_holds_the_next_thirty_days_and_the_overdue_ones() {
        let graph = FakeGraph::returning(vec![
            obligation("ob-1", "pending", "2026-03-10", "100"),
            obligation("ob-2", "overdue", "2026-02-25", "200"),
            obligation("ob-3", "pending", "2026-06-30", "300"),
            obligation("ob-4", "compliant", "2026-03-05", "400"),
            obligation("ob-5", "pending", "sometime", "500"),
        ]);
        let answer = summary(
            &graph,
            &scope(),
            &json!({"company_id": "co-1", "today": "2026-03-01"}),
        )
        .expect("summarised");
        let upcoming = answer["upcoming_deadlines"].as_array().unwrap();
        let ids: Vec<&str> = upcoming.iter().map(|e| field(e, "obligation_id")).collect();
        assert_eq!(ids, vec!["ob-2", "ob-1"]);
        assert_eq!(upcoming[0]["days_until"], json!(-4));
        assert_eq!(answer["compliance_score"], json!(20.0));
        assert_eq!(answer["pending"], json!(3));
        assert_eq!(answer["overdue"], json!(1));
    }

    #[test]
    fn a_missing_company_is_refused_rather_than_summarising_everything() {
        let graph = FakeGraph::returning(vec![]);
        assert!(summary(&graph, &scope(), &json!({})).is_err());
    }
}
