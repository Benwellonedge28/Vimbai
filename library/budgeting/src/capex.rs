#![forbid(unsafe_code)]
//! Capital expenditure budgeting with priority-based funding,
//! ported from capital-expenditure-budget-service `/prepare`.
//!
//! Investments, cash flows and the remaining budget are decimals, so a programme of
//! large projects is funded against the exact figure rather than a float that drifts
//! as each project is deducted.
//!
//! Two rules of the service are kept deliberately:
//!
//! * projects are funded in ascending priority while the remaining budget covers the
//!   whole investment; one that does not fit is deferred, never part-funded;
//! * `expected_roi` averages every project, funded or deferred, because the service
//!   divides the running total by the count it was given.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vimbai_common::Money;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapexProject {
    pub project_id: String,
    pub project_name: String,
    /// Amounts travel as decimal strings on the wire.
    pub initial_investment: Money,
    pub annual_cash_flow: Money,
    pub project_life: Money,
    /// Lower number = higher priority (Python sorts ascending).
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundedProject {
    pub project_id: String,
    pub project_name: String,
    pub initial_investment: Money,
    pub payback_period: Money,
    pub roi: Money,
    pub funded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeferredProject {
    pub project_id: String,
    pub project_name: String,
    pub initial_investment: Money,
    pub deferred_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapexResult {
    pub company_id: String,
    pub budget_year: String,
    pub total_capex_budget: Money,
    pub funded_projects: Vec<FundedProject>,
    pub deferred_projects: Vec<DeferredProject>,
    /// Mean payback of FUNDED projects (0 when none funded).
    pub total_payback_period: Money,
    /// Mean ROI across ALL projects, funded or not (0 when none).
    pub expected_roi: Money,
}

/// CapEx budget (capital-expenditure-budget-service `/prepare`).
pub fn prepare_capex_budget(
    company_id: &str,
    budget_year: &str,
    total_capex_budget: &Money,
    projects: &[CapexProject],
) -> CapexResult {
    let mut sorted: Vec<&CapexProject> = projects.iter().collect();
    // Python's sort is stable on priority only
    sorted.sort_by_key(|p| p.priority);

    let mut remaining = total_capex_budget.clone();
    let mut funded = Vec::new();
    let mut deferred = Vec::new();
    let mut total_roi = Money::zero();
    let mut payback_total = Money::zero();

    for p in &sorted {
        let payback = if p.annual_cash_flow.is_zero() {
            Money::zero()
        } else {
            p.initial_investment.over(&p.annual_cash_flow)
        };
        let roi = if p.initial_investment.is_zero() {
            Money::zero()
        } else {
            p.annual_cash_flow
                .times(&p.project_life)
                .minus(&p.initial_investment)
                .over(&p.initial_investment)
                .times(&Money::from_i64(100))
        };
        total_roi = total_roi.plus(&roi);

        if p.initial_investment <= remaining {
            let payback_period = payback.round(1);
            payback_total = payback_total.plus(&payback_period);
            funded.push(FundedProject {
                project_id: p.project_id.clone(),
                project_name: p.project_name.clone(),
                initial_investment: p.initial_investment.clone(),
                payback_period,
                roi: roi.round(2),
                funded: true,
            });
            remaining = remaining.minus(&p.initial_investment);
        } else {
            deferred.push(DeferredProject {
                project_id: p.project_id.clone(),
                project_name: p.project_name.clone(),
                initial_investment: p.initial_investment.clone(),
                deferred_reason: "Budget constraint".to_string(),
            });
        }
    }

    let total_payback_period = if funded.is_empty() {
        Money::zero()
    } else {
        payback_total
            .over(&Money::from_i64(funded.len() as i64))
            .round(1)
    };
    let expected_roi = if sorted.is_empty() {
        Money::zero()
    } else {
        total_roi
            .over(&Money::from_i64(sorted.len() as i64))
            .round(2)
    };

    CapexResult {
        company_id: company_id.to_string(),
        budget_year: budget_year.to_string(),
        total_capex_budget: total_capex_budget.clone(),
        funded_projects: funded,
        deferred_projects: deferred,
        total_payback_period,
        expected_roi,
    }
}

fn money_of(value: Option<&Value>) -> Result<Money, String> {
    match value {
        Some(Value::String(text)) => Money::parse(text).map_err(|e| e.0),
        Some(Value::Number(number)) => Money::from_f64_shortest(number.as_f64().unwrap_or(0.0))
            .map_err(|e| format!("{}: {e}", number)),
        Some(Value::Null) | None => Ok(Money::zero()),
        Some(other) => Err(format!("expected an amount, found {other}")),
    }
}

fn text_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The `/prepare` payload, as the service receives it.
pub fn prepare_from_json(payload: &Value) -> Result<Value, String> {
    let projects = payload
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut parsed = Vec::with_capacity(projects.len());
    for project in &projects {
        parsed.push(CapexProject {
            project_id: text_of(project, "project_id").to_string(),
            project_name: text_of(project, "project_name").to_string(),
            initial_investment: money_of(project.get("initial_investment"))?,
            annual_cash_flow: money_of(project.get("annual_cash_flow"))?,
            project_life: money_of(project.get("project_life"))?,
            priority: project.get("priority").and_then(Value::as_i64).unwrap_or(0),
        });
    }
    let result = prepare_capex_budget(
        text_of(payload, "company_id"),
        text_of(payload, "budget_year"),
        &money_of(payload.get("total_capex_budget"))?,
        &parsed,
    );
    serde_json::to_value(&result).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(text: &str) -> Money {
        Money::parse(text).unwrap()
    }

    fn p(id: &str, inv: &str, cf: &str, life: &str, pr: i64) -> CapexProject {
        CapexProject {
            project_id: id.into(),
            project_name: format!("Project {id}"),
            initial_investment: money(inv),
            annual_cash_flow: money(cf),
            project_life: money(life),
            priority: pr,
        }
    }

    #[test]
    fn priority_funding() {
        let projects = vec![
            p("A", "50000", "15000", "5", 2),
            p("B", "30000", "10000", "5", 1),
        ];
        let r = prepare_capex_budget("c1", "2027", &money("70000"), &projects);
        // B (priority 1) funded first: 30000, remaining 40000 < A's 50000
        assert_eq!(r.funded_projects.len(), 1);
        assert_eq!(r.funded_projects[0].project_id, "B");
        assert_eq!(r.funded_projects[0].payback_period.to_fixed(1), "3.0");
        // B roi = (50000-30000)/30000*100
        assert_eq!(r.funded_projects[0].roi.to_fixed(2), "66.67");
        assert_eq!(r.deferred_projects.len(), 1);
        assert_eq!(r.deferred_projects[0].project_id, "A");
        // expected_roi averages ALL projects
        assert_eq!(r.expected_roi.to_fixed(2), "58.33");
        assert_eq!(r.total_payback_period.to_fixed(1), "3.0");
    }

    #[test]
    fn deferral_when_budget_short() {
        let projects = vec![
            p("A", "50000", "15000", "5", 1),
            p("B", "30000", "10000", "5", 2),
        ];
        let r = prepare_capex_budget("c1", "2027", &money("40000"), &projects);
        // A (priority 1) costs 50000 > 40000 -> deferred even though first
        assert_eq!(r.funded_projects.len(), 1);
        assert_eq!(r.funded_projects[0].project_id, "B");
        assert_eq!(r.deferred_projects[0].project_id, "A");
        assert_eq!(r.deferred_projects[0].deferred_reason, "Budget constraint");
        assert_eq!(r.expected_roi.to_fixed(2), "58.33");
    }

    #[test]
    fn guards() {
        let r = prepare_capex_budget("c1", "2027", &money("1000"), &[]);
        assert!(r.total_payback_period.is_zero());
        assert!(r.expected_roi.is_zero());
        let zero_flow = vec![p("Z", "1000", "0", "5", 1)];
        let rz = prepare_capex_budget("c1", "2027", &money("5000"), &zero_flow);
        assert_eq!(rz.funded_projects[0].payback_period.to_fixed(1), "0.0");
        assert_eq!(rz.funded_projects[0].roi.to_fixed(2), "-100.00");
    }

    #[test]
    fn a_large_programme_leaves_exactly_the_last_cent() {
        // two cents of budget remain after the first project, so the second fits
        let projects = vec![
            p("A", "123456789012345678900.99", "1000", "2", 1),
            p("B", "0.02", "5", "2", 2),
        ];
        let r = prepare_capex_budget("c1", "2027", &money("123456789012345678901.01"), &projects);
        assert_eq!(r.funded_projects.len(), 2);
    }

    #[test]
    fn the_json_payload_round_trips() {
        let payload = serde_json::json!({
            "company_id": "c1",
            "budget_year": "2026",
            "total_capex_budget": "100000",
            "projects": [
                {"project_id": "a", "project_name": "A", "priority": 1,
                 "initial_investment": "50000", "annual_cash_flow": "15000", "project_life": "5"},
            ],
        });
        let out = prepare_from_json(&payload).unwrap();
        assert_eq!(out["funded_projects"][0]["roi"], "50.00");
        assert_eq!(out["expected_roi"], "50.00");
    }
}
