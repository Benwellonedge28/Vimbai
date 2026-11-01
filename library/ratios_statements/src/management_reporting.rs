//! management-reporting-service `/generate`: budget variance table,
//! performance summary, KPIs and recommendations.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

/// Ordered pairs preserve the Python dict iteration order of `actuals`.
pub type Items = Vec<(String, f64)>;

fn get(items: &Items, key: &str) -> f64 {
    items
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| *v)
        .unwrap_or(0.0)
}

fn get_or(items: &Items, key: &str, default: f64) -> f64 {
    items
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| *v)
        .unwrap_or(default)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VarianceLine {
    pub line_item: String,
    pub actual: f64,
    pub budget: f64,
    pub variance: f64,
    pub variance_pct: f64,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub total_revenue: f64,
    pub total_costs: f64,
    pub gross_margin: f64,
    pub ebitda: f64,
    pub net_margin: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportKpis {
    pub revenue_variance: f64,
    pub cost_variance: f64,
    pub forecast_accuracy: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManagementReport {
    pub company_id: String,
    pub period: String,
    pub variance_analysis: Vec<VarianceLine>,
    pub performance_summary: PerformanceSummary,
    pub kpis: ReportKpis,
    pub recommendations: Vec<String>,
}

/// PYTHON QUIRKS, all preserved:
/// - only keys present in `actuals` are compared, in that order;
/// - a missing budget line yields a 0% variance;
/// - `variance > 0` is Favorable, so a zero variance reads Unfavorable;
/// - KPI budget denominators default to 1, so a missing budget inflates
///   the percentage rather than erroring;
/// - forecast accuracy is the hardcoded 92.5.
pub fn generate_management_report(
    company_id: &str,
    period: &str,
    actuals: &Items,
    budget: &Items,
) -> ManagementReport {
    let mut variance_analysis = Vec::new();
    for (key, actual) in actuals.iter() {
        let budget_value = get(budget, key);
        let variance = *actual - budget_value;
        let variance_pct = if budget_value == 0.0 {
            0.0
        } else {
            variance / budget_value * 100.0
        };
        variance_analysis.push(VarianceLine {
            line_item: key.clone(),
            actual: py_round2(*actual),
            budget: py_round2(budget_value),
            variance: py_round2(variance),
            variance_pct: py_round2(variance_pct),
            status: if variance > 0.0 {
                "Favorable".to_string()
            } else {
                "Unfavorable".to_string()
            },
        });
    }

    let total_revenue = get(actuals, "revenue");
    let cogs = get(actuals, "cogs");
    let total_costs = cogs + get(actuals, "opex");
    let net_income = get(actuals, "net_income");

    let budget_revenue = get_or(budget, "revenue", 1.0);
    let budget_cogs = get_or(budget, "cogs", 1.0);
    let budget_opex = get_or(budget, "opex", 1.0);
    let budget_cost_total = budget_cogs + budget_opex;

    let recommendations: Vec<String> = {
        let count = variance_analysis
            .iter()
            .filter(|v| v.status == "Unfavorable" && v.variance_pct.abs() > 5.0)
            .count();
        if count > 0 {
            vec![format!(
                "{} items significantly over budget - review drivers",
                count
            )]
        } else {
            Vec::new()
        }
    };

    ManagementReport {
        company_id: company_id.to_string(),
        period: period.to_string(),
        variance_analysis,
        performance_summary: PerformanceSummary {
            total_revenue: py_round2(total_revenue),
            total_costs: py_round2(total_costs),
            gross_margin: py_round2(if total_revenue == 0.0 {
                0.0
            } else {
                (total_revenue - cogs) / total_revenue * 100.0
            }),
            ebitda: py_round2(total_revenue - total_costs),
            net_margin: py_round2(if total_revenue == 0.0 {
                0.0
            } else {
                net_income / total_revenue * 100.0
            }),
        },
        kpis: ReportKpis {
            revenue_variance: py_round2((total_revenue - budget_revenue) / budget_revenue * 100.0),
            cost_variance: py_round2((total_costs - budget_cost_total) / budget_cost_total * 100.0),
            forecast_accuracy: 92.5,
        },
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(items: &[(&str, f64)]) -> Items {
        items.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn actuals() -> Items {
        pairs(&[
            ("revenue", 1_100_000.0),
            ("cogs", 600_000.0),
            ("opex", 250_000.0),
            ("net_income", 120_000.0),
            ("marketing", 60_000.0),
        ])
    }

    fn budget() -> Items {
        pairs(&[
            ("revenue", 1_000_000.0),
            ("cogs", 600_000.0),
            ("opex", 200_000.0),
            ("net_income", 150_000.0),
            ("marketing", 40_000.0),
        ])
    }

    #[test]
    fn variance_table_follows_actuals_order() {
        let r = generate_management_report("c1", "2026-Q1", &actuals(), &budget());
        assert_eq!(r.variance_analysis.len(), 5);
        assert_eq!(r.variance_analysis[0].line_item, "revenue");
        assert_eq!(r.variance_analysis[0].variance, 100_000.0);
        assert_eq!(r.variance_analysis[0].variance_pct, 10.0);
        assert_eq!(r.variance_analysis[0].status, "Favorable");
        assert_eq!(r.variance_analysis[1].variance, 0.0);
        // a zero variance counts as Unfavorable
        assert_eq!(r.variance_analysis[1].status, "Unfavorable");
        assert_eq!(r.variance_analysis[3].variance_pct, -20.0);
    }

    #[test]
    fn performance_and_kpis() {
        let r = generate_management_report("c1", "2026-Q1", &actuals(), &budget());
        assert_eq!(r.performance_summary.total_revenue, 1_100_000.0);
        assert_eq!(r.performance_summary.total_costs, 850_000.0);
        assert_eq!(r.performance_summary.ebitda, 250_000.0);
        assert_eq!(r.performance_summary.gross_margin, 45.45);
        assert_eq!(r.performance_summary.net_margin, 10.91);
        assert_eq!(r.kpis.revenue_variance, 10.0);
        assert_eq!(r.kpis.cost_variance, 6.25);
        assert_eq!(r.kpis.forecast_accuracy, 92.5);
    }

    #[test]
    fn recommendations_count_overruns_beyond_five_percent() {
        let r = generate_management_report("c1", "2026-Q1", &actuals(), &budget());
        // Only net income is both Unfavorable and beyond 5%; the opex overrun
        // is labelled Favorable by the generic sign test, so it is not counted.
        assert_eq!(
            r.recommendations,
            vec!["1 items significantly over budget - review drivers".to_string()]
        );
        assert_eq!(r.variance_analysis[2].line_item, "opex");
        assert_eq!(r.variance_analysis[2].status, "Favorable");
        let on_budget = generate_management_report("c1", "2026-Q1", &budget(), &budget());
        assert!(on_budget.recommendations.is_empty());
    }

    #[test]
    fn missing_budget_lines_default_to_zero_and_one() {
        let actuals = pairs(&[("revenue", 500.0), ("royalties", 40.0)]);
        let r = generate_management_report("c1", "2026", &actuals, &Items::new());
        // no budget line at all: 0% variance and Favorable on any positive line
        assert_eq!(r.variance_analysis[1].variance_pct, 0.0);
        assert_eq!(r.variance_analysis[1].status, "Favorable");
        // KPI denominators default to 1, so percentages blow up rather than error
        assert_eq!(r.kpis.revenue_variance, 49_900.0);
        assert_eq!(r.kpis.cost_variance, -100.0);
    }
}
