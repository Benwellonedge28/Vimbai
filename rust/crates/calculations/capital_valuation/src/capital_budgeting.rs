//! capital-budgeting-service: appraise a slate of investment projects at once
//! and rank what to fund.
//!
//! PYTHON QUIRK: the "irr" here is not an internal rate of return. It is a
//! straight-line nudge from the cost of capital, `rate + npv / investment * 0.1`,
//! which says nothing about the rate that zeroes the NPV. The acceptance rule
//! still hinges on NPV and the profitability index, so the ranking is sound; the
//! figure reported beside each project is not. Preserved as it stands, with the
//! real [`crate::irr::irr`] available separately.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub initial_investment: f64,
    #[serde(default)]
    pub cash_flows: Vec<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapitalBudgetRequest {
    #[serde(default)]
    pub company_id: String,
    #[serde(default)]
    pub planning_period: String,
    #[serde(default)]
    pub cost_of_capital: f64,
    /// Accepted and ignored by the service: the slate is never trimmed to it.
    #[serde(default)]
    pub capital_budget_limit: f64,
    #[serde(default)]
    pub projects: Vec<ProjectInput>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppraisedProject {
    pub project_id: String,
    pub initial_investment: f64,
    pub cash_flows: Vec<f64>,
    pub npv: f64,
    /// The service's linear stand-in, not a true IRR.
    pub irr: f64,
    /// Whole periods as counted by the service, so an un-recovered project
    /// reports the length of its flow list rather than a missing value. A float
    /// because the service's model declares it one.
    pub payback_period: f64,
    pub profitability_index: f64,
    pub accept: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapitalBudgetResult {
    pub company_id: String,
    pub planning_period: String,
    pub projects_analyzed: Vec<AppraisedProject>,
    /// The five highest-NPV acceptable projects, best first.
    pub optimal_portfolio: Vec<String>,
    pub total_investment: f64,
    pub average_irr: f64,
    pub recommendations: Vec<String>,
}

fn appraise(project: &ProjectInput, cost_of_capital: f64) -> AppraisedProject {
    let investment = project.initial_investment;
    let flows = &project.cash_flows;

    let pv: f64 = flows
        .iter()
        .enumerate()
        .map(|(i, cf)| cf / (1.0 + cost_of_capital).powi(i as i32 + 1))
        .sum();
    let npv = pv - investment;
    let pi = if investment != 0.0 {
        pv / investment
    } else {
        0.0
    };

    let mut cumulative = 0.0;
    let mut payback = flows.len() as f64;
    for (i, cf) in flows.iter().enumerate() {
        cumulative += cf;
        if cumulative >= investment {
            payback = (i + 1) as f64;
            break;
        }
    }

    let irr = if investment != 0.0 {
        cost_of_capital + (npv / investment * 0.1)
    } else {
        0.0
    };

    AppraisedProject {
        project_id: project.id.clone(),
        initial_investment: investment,
        cash_flows: flows.clone(),
        npv: py_round(npv, 2),
        irr: py_round(irr, 4),
        payback_period: payback,
        profitability_index: py_round(pi, 4),
        accept: npv > 0.0 && pi > 1.0,
    }
}

pub fn analyse_capital_budget(req: &CapitalBudgetRequest) -> CapitalBudgetResult {
    let mut analysed = Vec::with_capacity(req.projects.len());
    let mut total_investment = 0.0;
    let mut irr_values = Vec::with_capacity(req.projects.len());

    for project in &req.projects {
        total_investment += project.initial_investment;
        let appraised = appraise(project, req.cost_of_capital);
        irr_values.push(appraised.irr);
        analysed.push(appraised);
    }

    // ranked by the rounded NPV, ties keeping the caller's order
    let mut ranked: Vec<&AppraisedProject> = analysed.iter().collect();
    ranked.sort_by(|a, b| {
        b.npv
            .partial_cmp(&a.npv)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let optimal: Vec<String> = ranked
        .into_iter()
        .filter(|p| p.accept)
        .take(5)
        .map(|p| p.project_id.clone())
        .collect();

    let average_irr = if irr_values.is_empty() {
        0.0
    } else {
        irr_values.iter().sum::<f64>() / irr_values.len() as f64
    };

    CapitalBudgetResult {
        company_id: req.company_id.clone(),
        planning_period: req.planning_period.clone(),
        projects_analyzed: analysed,
        optimal_portfolio: optimal,
        total_investment: py_round(total_investment, 2),
        average_irr: py_round(average_irr, 4),
        recommendations: vec![
            "Focus on high NPV projects within budget constraints".to_string(),
            "Consider project interdependencies".to_string(),
            "Review IRR against cost of capital".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> CapitalBudgetRequest {
        CapitalBudgetRequest {
            company_id: "c1".to_string(),
            planning_period: "2026".to_string(),
            cost_of_capital: 0.1,
            capital_budget_limit: 200_000.0,
            projects: vec![
                ProjectInput {
                    id: "p1".to_string(),
                    initial_investment: 100_000.0,
                    cash_flows: vec![40_000.0, 40_000.0, 40_000.0],
                },
                ProjectInput {
                    id: "p2".to_string(),
                    initial_investment: 50_000.0,
                    cash_flows: vec![10_000.0],
                },
            ],
        }
    }

    #[test]
    fn npv_discounts_each_flow_from_period_one() {
        let result = analyse_capital_budget(&request());
        // 40000/1.1 + 40000/1.21 + 40000/1.331 = 99474.07..., so npv = -525.93
        assert_eq!(result.projects_analyzed[0].npv, -525.92);
        // 10000/1.1 = 9090.91, so npv = -40909.09
        assert_eq!(result.projects_analyzed[1].npv, -40909.09);
    }

    #[test]
    fn the_reported_irr_is_a_linear_stand_in() {
        let result = analyse_capital_budget(&request());
        // 0.1 + (-525.92 / 100000 * 0.1) = 0.0995
        assert_eq!(result.projects_analyzed[0].irr, 0.0995);
        // a zero investment reports zero rather than dividing
        assert_eq!(result.average_irr, 0.0588);
    }

    #[test]
    fn payback_counts_whole_periods_and_falls_back_to_the_list_length() {
        let result = analyse_capital_budget(&request());
        // 40000 + 40000 + 40000 crosses 100000 only on the third flow
        assert_eq!(result.projects_analyzed[0].payback_period, 3.0);
        // the second project never recovers, so it reports one period
        assert_eq!(result.projects_analyzed[1].payback_period, 1.0);
    }

    #[test]
    fn only_positive_npv_above_index_one_is_accepted() {
        let mut req = request();
        req.projects[0].cash_flows = vec![50_000.0, 50_000.0, 50_000.0];
        let result = analyse_capital_budget(&req);
        let best = &result.projects_analyzed[0];
        assert!(best.npv > 0.0);
        assert!(best.profitability_index > 1.0);
        assert!(best.accept);
        assert_eq!(result.optimal_portfolio, vec!["p1".to_string()]);
        assert_eq!(result.total_investment, 150_000.0);
    }

    #[test]
    fn the_portfolio_stops_at_five_projects() {
        let req = CapitalBudgetRequest {
            cost_of_capital: 0.1,
            projects: (0..7)
                .map(|i| ProjectInput {
                    id: format!("p{i}"),
                    initial_investment: 1_000.0,
                    cash_flows: vec![5_000.0],
                })
                .collect(),
            ..Default::default()
        };
        let result = analyse_capital_budget(&req);
        assert_eq!(result.optimal_portfolio.len(), 5);
    }

    #[test]
    fn an_empty_slate_reports_zero() {
        let result = analyse_capital_budget(&CapitalBudgetRequest::default());
        assert_eq!(result.total_investment, 0.0);
        assert_eq!(result.average_irr, 0.0);
        assert!(result.optimal_portfolio.is_empty());
    }
}
