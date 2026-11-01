#![forbid(unsafe_code)]
//! Capital expenditure budgeting with priority-based funding,
//! ported from capital-expenditure-budget-service `/prepare`.

use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, py_round2};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapexProject {
    pub project_id: String,
    pub project_name: String,
    pub initial_investment: f64,
    pub annual_cash_flow: f64,
    pub project_life: f64,
    /// Lower number = higher priority (Python sorts ascending).
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundedProject {
    pub project_id: String,
    pub project_name: String,
    pub initial_investment: f64,
    pub payback_period: f64,
    pub roi: f64,
    pub funded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeferredProject {
    pub project_id: String,
    pub project_name: String,
    pub initial_investment: f64,
    pub deferred_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapexResult {
    pub company_id: String,
    pub budget_year: String,
    pub total_capex_budget: f64,
    pub funded_projects: Vec<FundedProject>,
    pub deferred_projects: Vec<DeferredProject>,
    /// Mean payback of FUNDED projects (0 when none funded).
    pub total_payback_period: f64,
    /// Mean ROI across ALL projects, funded or not (0 when none).
    pub expected_roi: f64,
}

/// CapEx budget (capital-expenditure-budget-service `/prepare`).
/// Projects are funded in ascending `priority` order while budget
/// remains; `expected_roi` averages every project.
pub fn prepare_capex_budget(
    company_id: &str,
    budget_year: &str,
    total_capex_budget: f64,
    projects: &[CapexProject],
) -> CapexResult {
    let mut sorted: Vec<&CapexProject> = projects.iter().collect();
    // Python's sort is stable on priority only
    sorted.sort_by_key(|p| p.priority);

    let mut remaining = total_capex_budget;
    let mut funded = Vec::new();
    let mut deferred = Vec::new();
    let mut total_roi = 0.0;
    for p in &sorted {
        let payback = if p.annual_cash_flow != 0.0 {
            p.initial_investment / p.annual_cash_flow
        } else {
            0.0
        };
        let total_return = p.annual_cash_flow * p.project_life;
        let roi = if p.initial_investment != 0.0 {
            (total_return - p.initial_investment) / p.initial_investment * 100.0
        } else {
            0.0
        };
        total_roi += roi;
        if p.initial_investment <= remaining {
            funded.push(FundedProject {
                project_id: p.project_id.clone(),
                project_name: p.project_name.clone(),
                initial_investment: p.initial_investment,
                payback_period: py_round(payback, 1),
                roi: py_round2(roi),
                funded: true,
            });
            remaining -= p.initial_investment;
        } else {
            deferred.push(DeferredProject {
                project_id: p.project_id.clone(),
                project_name: p.project_name.clone(),
                initial_investment: p.initial_investment,
                deferred_reason: "Budget constraint".to_string(),
            });
        }
    }

    let total_payback_period = if funded.is_empty() {
        0.0
    } else {
        let sum: f64 = funded.iter().map(|f| f.payback_period).sum();
        py_round(sum / funded.len() as f64, 1)
    };
    let expected_roi = if sorted.is_empty() {
        0.0
    } else {
        py_round2(total_roi / sorted.len() as f64)
    };

    CapexResult {
        company_id: company_id.to_string(),
        budget_year: budget_year.to_string(),
        total_capex_budget,
        funded_projects: funded,
        deferred_projects: deferred,
        total_payback_period,
        expected_roi,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: &str, inv: f64, cf: f64, life: f64, pr: i64) -> CapexProject {
        CapexProject {
            project_id: id.into(),
            project_name: format!("Project {id}"),
            initial_investment: inv,
            annual_cash_flow: cf,
            project_life: life,
            priority: pr,
        }
    }

    #[test]
    fn priority_funding() {
        let projects = vec![
            p("A", 50000.0, 15000.0, 5.0, 2),
            p("B", 30000.0, 10000.0, 5.0, 1),
        ];
        let r = prepare_capex_budget("c1", "2027", 70000.0, &projects);
        // B (priority 1) funded first: 30000, remaining 40000 < A's 50000
        assert_eq!(r.funded_projects.len(), 1);
        assert_eq!(r.funded_projects[0].project_id, "B");
        assert_eq!(r.funded_projects[0].payback_period, 3.0);
        // B roi = (50000-30000)/30000*100
        assert_eq!(
            r.funded_projects[0].roi,
            py_round2(20000.0 / 30000.0 * 100.0)
        );
        assert_eq!(r.deferred_projects.len(), 1);
        assert_eq!(r.deferred_projects[0].project_id, "A");
        // expected_roi averages ALL projects
        assert_eq!(
            r.expected_roi,
            py_round2((20000.0 / 30000.0 * 100.0 + 50.0) / 2.0)
        );
    }

    #[test]
    fn deferral_when_budget_short() {
        let projects = vec![
            p("A", 50000.0, 15000.0, 5.0, 1),
            p("B", 30000.0, 10000.0, 5.0, 2),
        ];
        let r = prepare_capex_budget("c1", "2027", 40000.0, &projects);
        // A (priority 1) costs 50000 > 40000 -> deferred even though first
        assert_eq!(r.funded_projects.len(), 1);
        assert_eq!(r.funded_projects[0].project_id, "B");
        assert_eq!(r.deferred_projects.len(), 1);
        assert_eq!(r.deferred_projects[0].project_id, "A");
        assert_eq!(r.deferred_projects[0].deferred_reason, "Budget constraint");
        // expected_roi averages ALL projects
        assert_eq!(r.expected_roi, py_round2((50.0 + 66.66666666666667) / 2.0));
    }

    #[test]
    fn guards() {
        let r = prepare_capex_budget("c1", "2027", 1000.0, &[]);
        assert_eq!(r.total_payback_period, 0.0);
        assert_eq!(r.expected_roi, 0.0);
        let zero_flow = vec![p("Z", 1000.0, 0.0, 5.0, 1)];
        let rz = prepare_capex_budget("c1", "2027", 5000.0, &zero_flow);
        assert_eq!(rz.funded_projects[0].payback_period, 0.0);
    }
}
