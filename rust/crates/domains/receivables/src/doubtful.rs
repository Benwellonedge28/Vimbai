#![forbid(unsafe_code)]
//! provision-doubtful-debts-service: a provision graded per debtor and escalated by lateness.
//!
//! Each debtor carries a grade (low, medium, high, doubtful) and the rate that grade attracts, plus an
//! outstanding amount. A calculation works out how late each debtor is and moves it up the grades if it is
//! late enough, then applies the rate it now carries and keeps the figures.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * the ageing ladder escalates and never relaxes: a debtor graded doubtful stays doubtful however current
//!   it is, and one graded low is lifted to high once it is ninety days late;
//! * the rates are the shipped ones - low 1%, medium 5%, high 25%, doubtful 75% - and a grade the service does
//!   not know earns the medium rate, which is what its lookup returned for anything unfamiliar;
//! * an assessment is a decision by the caller, so it sets the grade and the rate from the grade outright,
//!   including downwards - the escalation rule belongs to the calculation, not to the assessment;
//! * a provision is made from the entries the caller sends, summing the amounts those entries carry rather
//!   than recalculating them, so what is recorded is what was agreed;
//! * days overdue count from the due date and never go below nil.

use serde_json::{json, Value};

use crate::rates::{share_of, stored_rate};
use vimbai_common::dates::{day_of, days_between};
use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, refused, text};
use vimbai_store::write::remember;
use vimbai_store::{BookScope, Graph, Records, StoreError, DOUBTFUL_DEBTOR, DOUBTFUL_PROVISION};

/// The rate each grade attracts. A grade the service does not know is medium, as its lookup treated it.
fn rate_for(grade: &str) -> Money {
    let spelled = match grade {
        "low" => "1",
        "high" => "25",
        "doubtful" => "75",
        _ => "5",
    };
    Money::parse(spelled).unwrap_or_else(|_| Money::zero())
}

/// The ageing ladder: it escalates a grade, and never relaxes one.
fn escalated(days_overdue: u32, grade: &str, percent: &Money) -> (String, Money) {
    if days_overdue >= 180 {
        return ("doubtful".to_owned(), rate_for("doubtful"));
    }
    if days_overdue >= 90 {
        return ("high".to_owned(), rate_for("high"));
    }
    if days_overdue >= 60 {
        return ("medium".to_owned(), rate_for("medium"));
    }
    (grade.to_owned(), percent.clone())
}

fn debtor_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debtor_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, DOUBTFUL_DEBTOR).get(scope, debtor_id)
}

/// `POST /debtors/add`.
pub fn add_debtor(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "debtor_name");
    if name.is_empty() {
        return Err(refused("debtor_name is required"));
    }
    let amount = amount_or_zero(payload, "amount")?;
    let grade = match text(payload, "risk_assessment") {
        given if given.is_empty() => "medium".to_owned(),
        given => given,
    };
    let rate = rate_for(&grade);

    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("id");
        fields.insert("outstanding".into(), wire(&amount));
        fields.insert("risk_assessment".into(), json!(grade));
        fields.insert("provision_percent".into(), wire(&rate));
        fields.insert("provision_amount".into(), wire(&share_of(&amount, &rate)));
        fields.entry("days_overdue").or_insert(json!(0));
    } else {
        return Err(refused("a debtor must be an object"));
    }
    remember(graph, scope, DOUBTFUL_DEBTOR, body)
}

/// `POST /debtors/{debtor_id}/assess`.
pub fn assess_debtor(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    debtor_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let debtor = debtor_of(graph, scope, debtor_id)?;
    let grade = match text(payload, "risk_assessment") {
        given if given.is_empty() => field(&debtor, "risk_assessment").to_owned(),
        given => given,
    };
    let rate = rate_for(&grade);
    let outstanding = stored_amount(&debtor, "outstanding").unwrap_or_else(|_| Money::zero());

    let mut patch = json!({
        "risk_assessment": grade,
        "provision_percent": wire(&rate),
        "provision_amount": wire(&share_of(&outstanding, &rate)),
    });
    let notes = text(payload, "notes");
    if !notes.is_empty() {
        if let Some(fields) = patch.as_object_mut() {
            fields.insert("notes".into(), json!(notes));
        }
    }
    Records::new(graph, DOUBTFUL_DEBTOR).update(scope, debtor_id, &patch)?;
    debtor_of(graph, scope, debtor_id)
}

/// `POST /calculate-provision`.
pub fn calculate_provision(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    query: &Value,
) -> Result<Value, StoreError> {
    let period = text(query, "period_end");
    if period.is_empty() {
        return Err(refused("period_end is required"));
    }
    let kind = match text(query, "provision_type") {
        given if given.is_empty() => "specific".to_owned(),
        given => given,
    };

    let debtors = Records::new(graph, DOUBTFUL_DEBTOR).list(scope, 2000)?;
    let mut total = Money::zero();
    let mut outstanding = Money::zero();
    let mut entries = Vec::new();

    for debtor in debtors {
        let due = field(&debtor, "due_date").to_owned();
        let days = days_between(&period, &due).map(|days| days.max(0) as u32);
        let overdue = match (day_of(&period), day_of(&due)) {
            (Some(_), Some(_)) => days.unwrap_or(0),
            _ => 0,
        };
        let carried = stored_rate(&debtor, "provision_percent", "0")?;
        let (grade, rate) = escalated(overdue, field(&debtor, "risk_assessment"), &carried);
        let owed = stored_amount(&debtor, "outstanding").unwrap_or_else(|_| Money::zero());
        let amount = share_of(&owed, &rate);
        total = total.plus(&amount);
        outstanding = outstanding.plus(&owed);

        Records::new(graph, DOUBTFUL_DEBTOR).update(
            scope,
            field(&debtor, "id"),
            &json!({
                "days_overdue": overdue,
                "risk_assessment": grade,
                "provision_percent": wire(&rate),
                "provision_amount": wire(&amount),
            }),
        )?;

        entries.push(json!({
            "debtor_id": field(&debtor, "debtor_id"),
            "debtor_name": field(&debtor, "debtor_name"),
            "outstanding": wire(&owed),
            "days_overdue": overdue,
            "risk_assessment": grade,
            "provision_percent": wire(&rate),
            "provision_amount": wire(&amount),
        }));
    }

    Ok(json!({
        "period_end": period,
        "provision_type": kind,
        "total_debtors": entries.len(),
        "total_outstanding": wire(&outstanding),
        "total_provision_required": wire(&total),
        "entries": entries,
    }))
}

/// `POST /provision/create`.
pub fn create_provision(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let period = text(payload, "period_end");
    if period.is_empty() {
        return Err(refused("period_end is required"));
    }
    let opening = amount_or_zero(payload, "provision_opening")?;
    let kind = match text(payload, "provision_type") {
        given if given.is_empty() => "specific".to_owned(),
        given => given,
    };
    let entries = payload
        .get("entries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // what the entries say they need is what is provided, without recalculating it
    let required = entries
        .iter()
        .filter_map(|entry| stored_amount(entry, "provision_amount").ok())
        .fold(Money::zero(), |total, amount| total.plus(&amount));
    let adjustment = required.minus(&opening);

    let kept = remember(
        graph,
        scope,
        DOUBTFUL_PROVISION,
        json!({
            "period_end": period,
            "provision_type": kind,
            "total_debtors": entries.len(),
            "provision_required": wire(&required),
            "provision_opening": wire(&opening),
            "provision_adjustment": wire(&adjustment),
            "provision_closing": wire(&required),
            "entries": entries,
        }),
    )?;
    Ok(kept)
}

/// `GET /provisions`.
pub fn list_provisions(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let provisions = Records::new(graph, DOUBTFUL_PROVISION).list(scope, 2000)?;
    Ok(json!({"provisions": provisions, "count": provisions.len()}))
}

/// `GET /debtors`.
pub fn list_debtors(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let debtors = Records::new(graph, DOUBTFUL_DEBTOR).list(scope, 2000)?;
    Ok(json!({"debtors": debtors, "count": debtors.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn the_ladder_lifts_a_grade_and_never_lowers_one() {
        let low = rate_for("low");
        let doubtful = rate_for("doubtful");
        assert_eq!(escalated(90, "low", &low).0, "high");
        assert_eq!(escalated(0, "doubtful", &doubtful).0, "doubtful");
        assert_eq!(
            escalated(0, "high", &rate_for("high")).1.to_fixed(2),
            "25.00"
        );
        assert_eq!(escalated(200, "low", &low).1.to_fixed(2), "75.00");
    }

    #[test]
    fn an_unknown_grade_earns_the_medium_rate() {
        assert_eq!(rate_for("unheard of").to_fixed(2), "5.00");
    }

    #[test]
    fn a_debtor_is_added_with_the_rate_its_grade_attracts() {
        let graph = FakeGraph::returning(vec![json!({"id": "dd-1"})]);
        let answer = add_debtor(
            &graph,
            &scope(),
            &json!({"debtor_id": "d-1", "debtor_name": "Ana", "amount": 2000,
                    "risk_assessment": "high", "due_date": "2026-01-01"}),
        )
        .expect("added");
        assert_eq!(answer["provision_percent"], json!("25.000000"));
        assert_eq!(answer["provision_amount"], json!("500.000000"));
        assert_eq!(answer["outstanding"], json!("2000.000000"));
    }

    #[test]
    fn an_assessment_sets_the_grade_from_the_caller_including_downwards() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "dd-1", "debtor_name": "Ana", "outstanding": "2000.000000",
                        "risk_assessment": "high", "provision_percent": "25.000000"}),
            ],
            vec![json!({"id": "dd-1"})],
            vec![json!({"id": "dd-1", "risk_assessment": "low", "provision_percent": "1.000000"})],
        ]);
        let answer = assess_debtor(
            &graph,
            &scope(),
            "dd-1",
            &json!({"risk_assessment": "low", "notes": "paid since"}),
        )
        .expect("assessed");
        assert_eq!(answer["risk_assessment"], json!("low"));
    }

    #[test]
    fn the_calculation_ages_from_the_due_date_and_provides_at_the_lifted_rate() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "dd-1", "debtor_id": "d-1", "debtor_name": "Ana",
                        "outstanding": "1000.000000", "due_date": "2026-01-01",
                        "risk_assessment": "low", "provision_percent": "1.000000"}),
            ],
            vec![json!({"id": "dd-1"})],
        ]);
        let answer = calculate_provision(&graph, &scope(), &json!({"period_end": "2026-04-01"}))
            .expect("calculated");
        assert_eq!(answer["entries"][0]["days_overdue"], json!(90));
        assert_eq!(answer["total_provision_required"], json!("250.000000"));
        assert_eq!(answer["provision_type"], json!("specific"));
    }

    #[test]
    fn a_debtor_not_yet_due_is_never_overdue() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "dd-2", "debtor_id": "d-2", "outstanding": "1000.000000",
                        "due_date": "2026-06-01", "risk_assessment": "medium",
                        "provision_percent": "5.000000"}),
            ],
            vec![json!({"id": "dd-2"})],
        ]);
        let answer = calculate_provision(&graph, &scope(), &json!({"period_end": "2026-04-01"}))
            .expect("calculated");
        assert_eq!(answer["entries"][0]["days_overdue"], json!(0));
        assert_eq!(answer["entries"][0]["risk_assessment"], json!("medium"));
    }

    #[test]
    fn a_provision_sums_the_entries_it_is_given() {
        let graph = FakeGraph::returning(vec![
            json!({"id": "dp-1", "provision_required": "300.000000"}),
        ]);
        let answer = create_provision(
            &graph,
            &scope(),
            &json!({"period_end": "2026-04-01", "provision_opening": 100,
            "entries": [
                {"debtor_id": "d-1", "provision_amount": "250.000000"},
                {"debtor_id": "d-2", "provision_amount": "50.000000"}
            ]}),
        )
        .expect("provided");
        assert_eq!(answer["provision_required"], json!("300.000000"));
        assert_eq!(answer["provision_adjustment"], json!("200.000000"));
        assert_eq!(answer["provision_closing"], json!("300.000000"));
    }
}
