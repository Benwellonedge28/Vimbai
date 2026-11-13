//! Study endpoints: project ranking, share options, related parties, events
//! after the reporting period and household finance. Ported from
//! profitability-index-service, share-options-service, related-party-service,
//! events-after-reporting-service and household-finance-service.
//!
//! Kept as shipped:
//!
//! * Profitability index discounts each cash flow by its period, ranks by PI,
//!   and answers the empty case with the sentence "No projects meet acceptance
//!   criteria" rather than an empty list.
//! * Share options separate equity- and cash-settled grants; a grant is
//!   dilutive only when it is equity settled and the share price beats the
//!   exercise price. The vesting factor is never reported.
//! * Related parties count only the natures and transactions that arrive; a
//!   transaction is "significant" past one million, and revenue counts
//!   subsidiary/associate relationships while expenses count management ones
//!   plus the management compensation.
//! * Events after reporting list ONLY the litigation updates as identified
//!   events; adjustments, significant non-adjusting events and the disclosure
//!   flag come from the events themselves.
//! * Household savings rate reads excellent at 20%, good at 10%, fair at 0%,
//!   and expenses bucket by category in arrival order.

use crate::{div, flag_on, num, r2, rows, text};
use serde_json::{json, Map, Value};

pub fn analyze_profitability_index(request: &Value) -> Value {
    let rate = num(request, "discount_rate");
    let mut project_analysis = Vec::new();
    let mut total_investment = 0.0;

    for project in rows(request, "projects") {
        let investment = num(project, "initial_investment");
        total_investment += investment;
        let mut pv = 0.0;
        for (index, flow) in project
            .get("cash_flows")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            pv += flow.as_f64().unwrap_or(0.0) / (1.0 + rate).powi(index as i32 + 1);
        }
        let pi = div(pv, investment);
        let npv = pv - investment;
        project_analysis.push(json!({
            "project_id": text(project, "id"),
            "initial_investment": investment,
            "pv_of_future_cash_flows": r2(pv),
            "profitability_index": vimbai_common::py_round(pi, 4),
            "npv": r2(npv),
            "accept": pi > 1.0,
        }));
    }

    // Stable descending sort, matching Python's sorted(reverse=True).
    let mut order: Vec<usize> = (0..project_analysis.len()).collect();
    order.sort_by(|&a, &b| {
        project_analysis[b]["profitability_index"]
            .as_f64()
            .unwrap_or(0.0)
            .partial_cmp(
                &project_analysis[a]["profitability_index"]
                    .as_f64()
                    .unwrap_or(0.0),
            )
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let ranked: Vec<&Value> = order.iter().map(|i| &project_analysis[*i]).collect();
    let ranking: Vec<Value> = ranked
        .iter()
        .enumerate()
        .map(|(index, p)| {
            json!({"rank": index + 1, "project": p["project_id"], "pi": p["profitability_index"]})
        })
        .collect();
    let optimal: Vec<Value> = ranked
        .iter()
        .filter(|p| p["accept"] == json!(true))
        .map(|p| p["project_id"].clone())
        .collect();

    json!({
        "company_id": text(request, "company_id"),
        "discount_rate": rate,
        "project_analysis": project_analysis,
        "total_investment_required": r2(total_investment),
        "ranking_by_pi": ranking,
        "optimal_portfolio": if optimal.is_empty() {
            json!(["No projects meet acceptance criteria"])
        } else {
            Value::Array(optimal)
        },
        "recommendations": [
            "Accept all projects with PI > 1.0",
            "Prioritize projects by PI ranking",
            "Consider risk-adjusted returns for projects with similar PI",
        ],
    })
}

pub fn analyze_share_options(request: &Value) -> Value {
    let mut grants = Vec::new();
    let mut total_cost = 0.0;
    let mut equity_expense = 0.0;
    let mut cash_expense = 0.0;
    let mut dilutive = 0i64;
    let share_price = num(request, "share_price");

    for grant in rows(request, "option_grants") {
        let options = grant
            .get("number_of_options")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let fair_value = num(grant, "fair_value");
        let vesting_period = num(grant, "vesting_period");
        let vesting_prob = {
            let raw = grant.get("vesting_probability").and_then(Value::as_f64);
            raw.unwrap_or(1.0)
        };
        let cash_settled = flag_on(grant, "cash_settled");
        let total_grant_cost = options as f64 * fair_value;
        let vesting_factor = total_grant_cost / vesting_period * vesting_prob;
        total_cost += total_grant_cost;
        if cash_settled {
            cash_expense += vesting_factor;
        } else {
            equity_expense += vesting_factor;
            if share_price > num(grant, "exercise_price") {
                dilutive += options;
            }
        }
        grants.push(json!({
            "grant_id": text(grant, "id"),
            "number_of_options": options,
            "fair_value_per_option": fair_value,
            "vesting_period": vesting_period,
            "exercise_price": num(grant, "exercise_price"),
            "total_compensation_cost": r2(total_grant_cost),
            "vesting_probability": vesting_prob,
        }));
    }

    json!({
        "company_id": text(request, "company_id"),
        "period": text(request, "period"),
        "option_grants": grants,
        "total_compensation_cost": r2(total_cost),
        "equity_settled_expense": r2(equity_expense),
        "cash_settled_expense": r2(cash_expense),
        "dilutive_effect": dilutive,
        "recommendations": [
            "Ensure fair value calculations are documented",
            "Review vesting conditions",
            "Update option pricing model assumptions",
        ],
    })
}

pub fn analyze_related_parties(request: &Value) -> Value {
    // Insertion order matters: the parties answer in the order identified.
    let mut parties: Vec<(String, Value)> = Vec::new();
    for party in rows(request, "identified_parties") {
        let id = text(party, "id");
        parties.push((
            id.clone(),
            json!({
                "party_id": id,
                "relationship": text(party, "relationship"),
                "party_name": text(party, "name"),
                "transactions_count": 0,
                "total_amount": 0.0,
                "nature_of_transactions": Vec::<String>::new(),
            }),
        ));
    }

    let mut significant = Vec::new();
    for txn in rows(request, "transactions") {
        let party_id = text(txn, "related_party_id");
        let amount = num(txn, "amount");
        let nature = text(txn, "nature");
        if let Some((_, entry)) = parties.iter_mut().find(|(id, _)| *id == party_id) {
            entry["transactions_count"] =
                json!(entry["transactions_count"].as_i64().unwrap_or(0) + 1);
            entry["total_amount"] = json!(entry["total_amount"].as_f64().unwrap_or(0.0) + amount);
            if !nature.is_empty() {
                let natures = entry["nature_of_transactions"].as_array_mut().unwrap();
                if !natures.iter().any(|n| n.as_str() == Some(nature.as_str())) {
                    natures.push(Value::String(nature.clone()));
                }
            }
            if amount > 1_000_000.0 {
                significant.push(json!({
                    "party": party_id,
                    "amount": amount,
                    "nature": nature,
                    "terms": text(txn, "terms"),
                }));
            }
        }
    }

    let mut revenue = 0.0;
    let mut expenses = 0.0;
    for (_, party) in &parties {
        let relationship = party["relationship"].as_str().unwrap_or("").to_lowercase();
        let total = party["total_amount"].as_f64().unwrap_or(0.0);
        if relationship.contains("subsidiary") || relationship.contains("associate") {
            revenue += total;
        }
        if relationship.contains("management") {
            expenses += total;
        }
    }
    expenses += num(request, "management_compensation");

    json!({
        "company_id": text(request, "company_id"),
        "audit_id": text(request, "audit_id"),
        "related_parties": parties.into_iter().map(|(_, p)| p).collect::<Vec<Value>>(),
        "total_related_party_revenue": r2(revenue),
        "total_related_party_expenses": r2(expenses),
        "significant_transactions": if significant.is_empty() {
            json!([{"message": "No individually significant transactions"}])
        } else {
            Value::Array(significant)
        },
        "arms_length_conclusion": if flag_on(request, "arms_length_comparison") {
            "Transfers at arm's length"
        } else {
            "Review of terms required"
        },
        "disclosure_completeness": "Complete",
        "recommendations": [
            "Ensure all related party relationships are disclosed",
            "Document arm's length nature of transactions",
            "Review and approve related party transactions by independent directors",
        ],
    })
}

pub fn analyze_events_after_reporting(request: &Value) -> Value {
    let mut adjusting_count = 0i64;
    let mut non_adjusting_count = 0i64;
    let mut adjusting_impact = 0.0;
    let mut significant = Vec::new();

    for event in rows(request, "events") {
        let impact = num(event, "financial_impact");
        if flag_on(event, "adjusting_event") {
            adjusting_count += 1;
            adjusting_impact += impact.abs();
        } else {
            non_adjusting_count += 1;
            if impact.abs() > 5_000_000.0 {
                significant.push(Value::String(text(event, "description")));
            }
        }
    }

    // QUIRK: only the litigation updates reach the identified-events list.
    let identified: Vec<Value> = rows(request, "litigation_updates")
        .iter()
        .map(|lit| {
            json!({
                "event_id": text(lit, "id"),
                "event_date": text(lit, "settlement_date"),
                "description": format!("Litigation settlement: {}", text(lit, "description")),
                "event_type": "legal",
                "adjusting": flag_on(lit, "adjusting"),
                "financial_impact": num(lit, "amount"),
            })
        })
        .collect();

    json!({
        "company_id": text(request, "company_id"),
        "audit_id": text(request, "audit_id"),
        "balance_sheet_date": text(request, "balance_sheet_date"),
        "authorization_date": text(request, "authorization_date"),
        "adjusting_events": adjusting_count,
        "non_adjusting_events": non_adjusting_count,
        "events_identified": identified,
        "total_adjusting_impact": r2(adjusting_impact),
        "significant_non_adjusting": if significant.is_empty() {
            json!(["No significant non-adjusting events"])
        } else {
            Value::Array(significant)
        },
        "disclosure_required": adjusting_count > 0 || non_adjusting_count > 0,
        "recommendations": [
            "Adjust financial statements for adjusting events",
            "Disclose significant non-adjusting events in notes",
            "Update going concern assessment if applicable",
            "Review subsequent events up to report authorization date",
        ],
    })
}

pub fn analyze_household(request: &Value) -> Value {
    let incomes = rows(request, "incomes");
    let expenses = rows(request, "expenses");
    let income: f64 = incomes.iter().map(|i| num(i, "amount")).sum();
    let expense_total: f64 = expenses.iter().map(|e| num(e, "amount")).sum();
    let surplus = income - expense_total;
    let savings_rate = div(surplus, income) * 100.0;

    let mut by_category: Map<String, Value> = Map::new();
    for expense in expenses {
        let key = text(expense, "category");
        let running = by_category.get(&key).and_then(Value::as_f64).unwrap_or(0.0);
        by_category.insert(key, json!(running + num(expense, "amount")));
    }
    let expense_by_category: Map<String, Value> = by_category
        .into_iter()
        .map(|(k, v)| (k, json!(r2(v.as_f64().unwrap_or(0.0)))))
        .collect();

    let assets: f64 = rows(request, "assets")
        .iter()
        .map(|a| num(a, "value"))
        .sum();
    let liabilities: f64 = rows(request, "liabilities")
        .iter()
        .map(|l| num(l, "amount"))
        .sum();

    let health = if savings_rate >= 20.0 {
        "excellent"
    } else if savings_rate >= 10.0 {
        "good"
    } else if savings_rate >= 0.0 {
        "fair"
    } else {
        "poor"
    };

    json!({
        "household_id": text(request, "household_id"),
        "period": text(request, "period"),
        "total_income": r2(income),
        "total_expenses": r2(expense_total),
        "surplus_deficit": r2(surplus),
        "savings_rate": vimbai_common::py_round(savings_rate, 1),
        "net_worth": r2(assets - liabilities),
        "expense_by_category": expense_by_category,
        "budget_health": health,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn normalise(value: &Value) -> Value {
        match value {
            Value::Number(n) => json!(n.as_f64().unwrap_or(0.0)),
            Value::Array(items) => Value::Array(items.iter().map(normalise).collect()),
            Value::Object(map) => {
                Value::Object(map.iter().map(|(k, v)| (k.clone(), normalise(v))).collect())
            }
            other => other.clone(),
        }
    }

    fn assert_parity(actual: Value, expected: &str) {
        let expected: Value = serde_json::from_str(expected).expect("expected json");
        assert_eq!(
            serde_json::to_string(&normalise(&actual)).unwrap(),
            serde_json::to_string(&normalise(&expected)).unwrap()
        );
    }

    #[test]
    fn profitability_index_matches_the_service() {
        let request = json!({"company_id": "C1", "discount_rate": 0.1, "projects": [
            {"id": "P1", "initial_investment": 1000, "cash_flows": [300, 400, 500]},
            {"id": "P2", "initial_investment": 500, "cash_flows": [100]}
        ]});
        let expected = r#"{
            "company_id": "C1",
            "discount_rate": 0.1,
            "project_analysis": [
                {"project_id": "P1", "initial_investment": 1000.0, "pv_of_future_cash_flows": 978.96,
                 "profitability_index": 0.979, "npv": -21.04, "accept": false},
                {"project_id": "P2", "initial_investment": 500.0, "pv_of_future_cash_flows": 90.91,
                 "profitability_index": 0.1818, "npv": -409.09, "accept": false}
            ],
            "total_investment_required": 1500.0,
            "ranking_by_pi": [{"rank": 1, "project": "P1", "pi": 0.979},
                              {"rank": 2, "project": "P2", "pi": 0.1818}],
            "optimal_portfolio": ["No projects meet acceptance criteria"],
            "recommendations": ["Accept all projects with PI > 1.0",
                                "Prioritize projects by PI ranking",
                                "Consider risk-adjusted returns for projects with similar PI"]
        }"#;
        assert_parity(analyze_profitability_index(&request), expected);
    }

    #[test]
    fn share_options_match_the_service() {
        let request = json!({"company_id": "C1", "period": "FY2026", "share_price": 60, "option_grants": [
            {"id": "G1", "number_of_options": 1000, "fair_value": 12, "vesting_period": 3,
             "vesting_probability": 0.9, "exercise_price": 50, "cash_settled": false},
            {"id": "G2", "number_of_options": 500, "fair_value": 8, "vesting_period": 2,
             "vesting_probability": 1.0, "exercise_price": 70, "cash_settled": true}
        ]});
        let expected = r#"{
            "company_id": "C1",
            "period": "FY2026",
            "option_grants": [
                {"grant_id": "G1", "number_of_options": 1000, "fair_value_per_option": 12.0,
                 "vesting_period": 3.0, "exercise_price": 50.0, "total_compensation_cost": 12000.0,
                 "vesting_probability": 0.9},
                {"grant_id": "G2", "number_of_options": 500, "fair_value_per_option": 8.0,
                 "vesting_period": 2.0, "exercise_price": 70.0, "total_compensation_cost": 4000.0,
                 "vesting_probability": 1.0}
            ],
            "total_compensation_cost": 16000.0,
            "equity_settled_expense": 3600.0,
            "cash_settled_expense": 2000.0,
            "dilutive_effect": 1000,
            "recommendations": ["Ensure fair value calculations are documented",
                                "Review vesting conditions",
                                "Update option pricing model assumptions"]
        }"#;
        assert_parity(analyze_share_options(&request), expected);
    }

    #[test]
    fn related_parties_match_the_service() {
        let request = json!({"company_id": "C1", "audit_id": "A1", "management_compensation": 50000,
            "arms_length_comparison": true,
            "identified_parties": [{"id": "P1", "relationship": "Subsidiary", "name": "Sub Co"},
                                   {"id": "P2", "relationship": "Key management", "name": "Boss"}],
            "transactions": [{"related_party_id": "P1", "amount": 1500000, "nature": "sales", "terms": "30d"},
                             {"related_party_id": "P1", "amount": 200000, "nature": "sales", "terms": "30d"},
                             {"related_party_id": "P2", "amount": 100000, "nature": "loan", "terms": "0%"}]});
        let out = analyze_related_parties(&request);
        assert_eq!(out["related_parties"][0]["transactions_count"], json!(2));
        assert_eq!(out["related_parties"][0]["total_amount"], json!(1700000.0));
        // the nature is listed once however many transactions carry it
        assert_eq!(
            out["related_parties"][0]["nature_of_transactions"],
            json!(["sales"])
        );
        assert_eq!(out["total_related_party_revenue"], json!(1700000.0));
        // management relationship plus the management compensation
        assert_eq!(out["total_related_party_expenses"], json!(150000.0));
        assert_eq!(
            out["significant_transactions"][0]["amount"],
            json!(1500000.0)
        );
        assert_eq!(out["significant_transactions"][0]["terms"], json!("30d"));
        assert_eq!(
            out["arms_length_conclusion"],
            json!("Transfers at arm's length")
        );
        assert_eq!(out["disclosure_completeness"], json!("Complete"));
    }

    #[test]
    fn no_significant_related_party_transactions_answers_with_a_note() {
        let request = json!({"company_id": "C1", "identified_parties": [],
                            "transactions": [], "arms_length_comparison": false});
        let out = analyze_related_parties(&request);
        assert_eq!(
            out["significant_transactions"],
            json!([{"message": "No individually significant transactions"}])
        );
        assert_eq!(
            out["arms_length_conclusion"],
            json!("Review of terms required")
        );
        assert_eq!(out["total_related_party_expenses"], json!(0.0));
    }

    #[test]
    fn events_after_reporting_match_the_service() {
        let request = json!({"company_id": "C1", "audit_id": "A1", "balance_sheet_date": "2026-06-30",
            "financial_statements_date": "2026-08-15", "authorization_date": "2026-09-01",
            "events": [{"adjusting_event": true, "financial_impact": -250000, "description": "Customer insolvency"},
                       {"adjusting_event": false, "financial_impact": 8000000, "description": "Factory fire"},
                       {"adjusting_event": false, "financial_impact": 100000, "description": "Small claim"}],
            "litigation_updates": [{"id": "L1", "settlement_date": "2026-07-15",
                                    "description": "Patent suit", "adjusting": true, "amount": -300000}]});
        let expected = r#"{
            "company_id": "C1",
            "audit_id": "A1",
            "balance_sheet_date": "2026-06-30",
            "authorization_date": "2026-09-01",
            "adjusting_events": 1,
            "non_adjusting_events": 2,
            "events_identified": [{"event_id": "L1", "event_date": "2026-07-15",
                                   "description": "Litigation settlement: Patent suit",
                                   "event_type": "legal", "adjusting": true,
                                   "financial_impact": -300000.0}],
            "total_adjusting_impact": 250000.0,
            "significant_non_adjusting": ["Factory fire"],
            "disclosure_required": true,
            "recommendations": ["Adjust financial statements for adjusting events",
                                "Disclose significant non-adjusting events in notes",
                                "Update going concern assessment if applicable",
                                "Review subsequent events up to report authorization date"]
        }"#;
        assert_parity(analyze_events_after_reporting(&request), expected);
    }

    #[test]
    fn household_finance_matches_the_service() {
        let request = json!({"household_id": "H1", "period": "2026-10",
            "incomes": [{"source": "salary", "amount": 2000}, {"source": "rent", "amount": 500}],
            "expenses": [{"category": "housing", "description": "rent", "amount": 800},
                         {"category": "food", "description": "groceries", "amount": 400},
                         {"category": "housing", "description": "rates", "amount": 200}],
            "assets": [{"name": "cash", "value": 5000}, {"name": "car", "value": 3000}],
            "liabilities": [{"name": "loan", "amount": 2000}]});
        let expected = r#"{
            "household_id": "H1",
            "period": "2026-10",
            "total_income": 2500.0,
            "total_expenses": 1400.0,
            "surplus_deficit": 1100.0,
            "savings_rate": 44.0,
            "net_worth": 6000.0,
            "expense_by_category": {"housing": 1000.0, "food": 400.0},
            "budget_health": "excellent"
        }"#;
        assert_parity(analyze_household(&request), expected);
    }

    #[test]
    fn household_without_income_reports_fair_health_and_zero_rate() {
        let request = json!({"household_id": "H2", "period": "2026-10", "incomes": [],
                            "expenses": [], "assets": [], "liabilities": []});
        let out = analyze_household(&request);
        assert_eq!(out["savings_rate"], json!(0.0));
        assert_eq!(out["budget_health"], json!("fair"));
        assert_eq!(out["net_worth"], json!(0.0));
    }
}
