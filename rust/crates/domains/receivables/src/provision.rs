#![forbid(unsafe_code)]
//! provision-bad-debts-service: a provision aged from a rule ladder, and the journal that moves it.
//!
//! Debtors are held with the days they have been outstanding and an outstanding amount. A calculation ages
//! each one into a rung of the policy ladder, applies that rung's rate, keeps the result on the debtor and
//! adds the results up. The journal then asks for the provision again and books the difference from what was
//! already provided.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * ageing starts from the debtor's stored days and adds the days since the period end, so running the
//!   same period twice ages the debtors twice - the shipped service did exactly that, and a single call
//!   answers what it answered;
//! * the ladder's rungs are the ages in days and the rate each carries, with the service's own defaults when
//!   no policy is given; a rung the policy names but gives no rate for falls back to the default rate;
//! * the caller's total debtors figure wins when it sends one, and is otherwise the sum of the debtors;
//! * the journal never threads a policy through, so it always ages against the default ladder - and its
//!   credit line carries the size of a negative adjustment, which is the shape the service booked;
//! * a rule the policy gives as text is read as the rate it spells.

use serde_json::{json, Value};

use crate::rates::share_of;
use vimbai_common::dates::days_between;
use vimbai_common::Money;
use vimbai_store::amounts::{amount_or_zero, stored_amount, wire};
use vimbai_store::read::{field, refused, text, whole};
use vimbai_store::write::remember;
use vimbai_store::{
    BookScope, Graph, Records, StoreError, DEBTOR_RECORD, PROVISION_JOURNAL, PROVISION_POLICY,
};

/// The ladder the service starts with: a rung per age in days, and the rate it carries.
const DEFAULT_LADDER: [(&str, &str); 4] = [("180", "50"), ("90", "10"), ("60", "5"), ("30", "1")];

fn ladder(policy: Option<&Value>) -> Value {
    match policy.and_then(|policy| policy.get("policy_rules")) {
        Some(rules) if rules.is_object() => rules.clone(),
        _ => json!({"0": 0, "30": 1, "60": 5, "90": 10, "180": 50, "365": 100}),
    }
}

/// Which rung an age falls in, and the status its name carries.
fn rung_for(days: u32, rules: &Value) -> (String, Money) {
    if days >= 365 {
        return ("bad".to_owned(), Money::from_i64(100));
    }
    for (threshold, fallback) in DEFAULT_LADDER {
        let age: u32 = threshold.parse().unwrap_or(0);
        if days >= age {
            let rate = rules
                .get(threshold)
                .map(|named| match named {
                    Value::String(spelled) => spelled.clone(),
                    other => other.to_string(),
                })
                .and_then(|named| Money::parse(&named).ok())
                .unwrap_or_else(|| Money::parse(fallback).unwrap_or_else(|_| Money::zero()));
            let status = match age {
                180 => "overdue_180",
                90 => "overdue_90",
                60 => "overdue_60",
                _ => "overdue_30",
            };
            return (status.to_owned(), rate);
        }
    }
    ("current".to_owned(), Money::zero())
}

fn policy_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    policy_id: &str,
) -> Result<Option<Value>, StoreError> {
    if policy_id.is_empty() {
        return Ok(None);
    }
    match Records::new(graph, PROVISION_POLICY).get(scope, policy_id) {
        Ok(policy) => Ok(Some(policy)),
        // the shipped service aged against the default ladder when the policy was not found
        Err(StoreError::NotFound) => Ok(None),
        Err(other) => Err(other),
    }
}

/// `POST /policies`.
pub fn create_provision_policy(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "policy_name");
    if name.is_empty() {
        return Err(refused("policy_name is required"));
    }
    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("id");
        let rules = fields.get("policy_rules").cloned().unwrap_or(Value::Null);
        if !rules.is_object() {
            fields.insert("policy_rules".into(), ladder(None));
        }
    } else {
        return Err(refused("a policy must be an object"));
    }
    remember(graph, scope, PROVISION_POLICY, body)
}

/// `GET /policies`.
pub fn list_provision_policies(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let policies = Records::new(graph, PROVISION_POLICY).list(scope, 2000)?;
    Ok(json!({"policies": policies, "count": policies.len()}))
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
    let outstanding = amount_or_zero(payload, "outstanding_amount")?;
    let mut body = payload.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("id");
        fields.insert("outstanding_amount".into(), wire(&outstanding));
        fields.entry("days_outstanding").or_insert(json!(0));
        fields.entry("status").or_insert(json!("current"));
        fields
            .entry("provision_percent")
            .or_insert(json!("0.000000"));
        fields.insert("provision_amount".into(), wire(&Money::zero()));
    } else {
        return Err(refused("a debtor must be an object"));
    }
    remember(graph, scope, DEBTOR_RECORD, body)
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
    // the port takes today from the request, so the same call answers the same figures twice
    let today = text(query, "today");
    let elapsed = if today.is_empty() {
        0
    } else {
        days_between(&today, &period).unwrap_or(0)
    };
    let policy = policy_of(graph, scope, &text(query, "policy_id"))?;
    let rules = ladder(policy.as_ref());

    let debtors = Records::new(graph, DEBTOR_RECORD).list(scope, 2000)?;
    let mut required = Money::zero();
    let mut ages: Vec<(String, Money)> = Vec::new();
    let mut entries = Vec::new();

    for debtor in debtors {
        let stored = whole(&debtor, "days_outstanding") as i64;
        let days = (elapsed + stored).max(0) as u32;
        let (status, rate) = rung_for(days, &rules);
        let outstanding =
            stored_amount(&debtor, "outstanding_amount").unwrap_or_else(|_| Money::zero());
        let amount = share_of(&outstanding, &rate);
        required = required.plus(&amount);

        Records::new(graph, DEBTOR_RECORD).update(
            scope,
            field(&debtor, "id"),
            &json!({
                "days_outstanding": days,
                "status": status,
                "provision_percent": wire(&rate),
                "provision_amount": wire(&amount),
            }),
        )?;

        ages.push((status.clone(), outstanding.clone()));
        entries.push(json!({
            "debtor_id": field(&debtor, "debtor_id"),
            "debtor_name": field(&debtor, "debtor_name"),
            "outstanding_amount": wire(&outstanding),
            "days_outstanding": days,
            "status": status,
            "provision_percent": wire(&rate),
            "provision_amount": wire(&amount),
        }));
    }

    let sent = amount_or_zero(query, "total_debtors")?;
    let total = if sent.is_zero() {
        entries
            .iter()
            .filter_map(|entry| Money::parse(&text(entry, "outstanding_amount")).ok())
            .fold(Money::zero(), |total, amount| total.plus(&amount))
    } else {
        sent
    };

    let aged = |wanted: &str| {
        wire(
            &ages
                .iter()
                .filter(|(status, _)| status == wanted)
                .fold(Money::zero(), |total, (_, outstanding)| {
                    total.plus(outstanding)
                }),
        )
    };

    Ok(json!({
        "period_end": period,
        "total_debtors": wire(&total),
        "provision_required": wire(&required),
        "entries": entries,
        "ageing_summary": {
            "current": aged("current"),
            "overdue_30": aged("overdue_30"),
            "overdue_60": aged("overdue_60"),
            "overdue_90": aged("overdue_90"),
            "overdue_180": aged("overdue_180"),
            "bad": aged("bad"),
        },
    }))
}

/// `POST /journal/create`.
pub fn create_journal(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let period = text(payload, "period_end");
    if period.is_empty() {
        return Err(refused("period_end is required"));
    }
    let opening = amount_or_zero(payload, "provision_opening")?;
    // no policy is threaded through here, so this always ages against the default ladder
    let calculation = calculate_provision(
        graph,
        scope,
        &json!({
            "period_end": period,
            "total_debtors": payload.get("total_debtors").cloned().unwrap_or(json!(0)),
            "today": text(payload, "today"),
        }),
    )?;
    let required =
        Money::parse(&text(&calculation, "provision_required")).unwrap_or_else(|_| Money::zero());
    let adjustment = required.minus(&opening);

    let (debit, credit) = if adjustment.is_negative() {
        (Money::zero(), adjustment.abs())
    } else {
        (adjustment.clone(), adjustment.clone())
    };

    let journal = json!({
        "date": period,
        "description": format!("Provision for doubtful debts as at {period}"),
        "entries": [
            {"account_code": "6300", "description": "Bad Debts Expense",
             "debit": wire(&debit), "credit": wire(&Money::zero())},
            {"account_code": "1310", "description": "Provision for Doubtful Debts",
             "debit": wire(&Money::zero()), "credit": wire(&credit)},
        ],
        "reference": format!("PROV-{period}"),
    });

    let kept = remember(
        graph,
        scope,
        PROVISION_JOURNAL,
        json!({
            "period_end": period,
            "total_debtors": calculation["total_debtors"].clone(),
            "provision_required": wire(&required),
            "provision_opening": wire(&opening),
            "provision_adjustment": wire(&adjustment),
            "journal": journal,
        }),
    )?;

    Ok(json!({
        "journal": kept,
        "provision_required": wire(&required),
        "provision_adjustment": wire(&adjustment),
        "provision_opening": wire(&opening),
    }))
}

/// `GET /provision-history`.
pub fn provision_history(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
) -> Result<Value, StoreError> {
    let journals = Records::new(graph, PROVISION_JOURNAL).list(scope, 2000)?;
    Ok(json!({"provisions": journals, "count": journals.len()}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn an_age_falls_in_the_rung_below_it() {
        let rules = ladder(None);
        assert_eq!(rung_for(0, &rules).0, "current");
        assert_eq!(rung_for(29, &rules).0, "current");
        assert_eq!(rung_for(30, &rules).1.to_fixed(2), "1.00");
        assert_eq!(rung_for(95, &rules).0, "overdue_90");
        assert_eq!(rung_for(365, &rules).0, "bad");
        assert_eq!(rung_for(365, &rules).1.to_fixed(2), "100.00");
    }

    #[test]
    fn a_rung_the_policy_names_wins_over_the_default() {
        let rules = json!({"30": "2.5", "60": 7});
        assert_eq!(rung_for(45, &rules).1.to_fixed(2), "2.50");
        assert_eq!(rung_for(70, &rules).1.to_fixed(2), "7.00");
    }

    #[test]
    fn the_calculation_ages_each_debtor_and_adds_the_rates_up() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "d-1", "debtor_id": "d-1", "debtor_name": "Ana",
                       "outstanding_amount": "1000.000000", "days_outstanding": 0}),
                json!({"id": "d-2", "debtor_id": "d-2", "debtor_name": "Bongani",
                       "outstanding_amount": "500.000000", "days_outstanding": 30}),
            ],
            vec![json!({"id": "d-1"})],
            vec![json!({"id": "d-2"})],
        ]);
        let answer = calculate_provision(
            &graph,
            &scope(),
            &json!({"period_end": "2026-03-01", "today": "2026-03-31"}),
        )
        .expect("calculated");
        // 1000 at 30 days is 1%, 500 at 60 days is 5%
        assert_eq!(answer["provision_required"], json!("35.000000"));
        assert_eq!(answer["entries"][0]["status"], json!("overdue_30"));
        assert_eq!(answer["entries"][1]["status"], json!("overdue_60"));
        assert_eq!(answer["ageing_summary"]["overdue_30"], json!("1000.000000"));
    }

    #[test]
    fn the_caller_total_wins_when_it_sends_one() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "d-1", "debtor_id": "d-1", "outstanding_amount": "100.000000",
                        "days_outstanding": 0}),
            ],
            vec![json!({"id": "d-1"})],
        ]);
        let answer = calculate_provision(
            &graph,
            &scope(),
            &json!({"period_end": "2026-03-01", "today": "2026-03-01", "total_debtors": 9999}),
        )
        .expect("calculated");
        assert_eq!(answer["total_debtors"], json!("9999.000000"));
    }

    #[test]
    fn a_journal_books_the_difference_from_what_was_already_provided() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "d-1", "debtor_id": "d-1", "outstanding_amount": "1000.000000",
                        "days_outstanding": 90}),
            ],
            vec![json!({"id": "d-1"})],
            vec![json!({"id": "p-1"})],
        ]);
        let answer = create_journal(
            &graph,
            &scope(),
            &json!({"period_end": "2026-03-01", "total_debtors": 1000, "provision_opening": 0}),
        )
        .expect("journalled");
        // 90 days against the default ladder is 10%
        assert_eq!(answer["provision_required"], json!("100.000000"));
        assert_eq!(answer["provision_adjustment"], json!("100.000000"));
        assert_eq!(
            answer["journal"]["journal"]["entries"][0]["debit"],
            json!("100.000000")
        );
    }

    #[test]
    fn a_negative_adjustment_is_credited_as_its_size() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "d-1", "debtor_id": "d-1", "outstanding_amount": "1000.000000",
                        "days_outstanding": 0}),
            ],
            vec![json!({"id": "d-1"})],
            vec![json!({"id": "p-1"})],
        ]);
        let answer = create_journal(
            &graph,
            &scope(),
            &json!({"period_end": "2026-03-01", "total_debtors": 1000, "provision_opening": 250}),
        )
        .expect("journalled");
        assert_eq!(answer["provision_adjustment"], json!("-250.000000"));
        let lines = &answer["journal"]["journal"]["entries"];
        assert_eq!(lines[0]["debit"], json!("0.000000"));
        assert_eq!(lines[1]["credit"], json!("250.000000"));
    }

    #[test]
    fn a_period_end_is_required_and_nothing_is_recorded_without_one() {
        let graph = FakeGraph::returning(vec![]);
        assert!(matches!(
            calculate_provision(&graph, &scope(), &json!({})),
            Err(StoreError::Invalid(_))
        ));
    }
}
