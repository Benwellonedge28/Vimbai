//! management-reporting-service: budget variance, headline performance and the
//! handful of KPIs a management pack leads with.
//!
//! PYTHON QUIRK: the variance lines follow the order the caller's actuals
//! arrive in, not a fixed order, so the report is built from an ordered map.
//! Two budgets default to 1 rather than 0, which is why an empty budget gives a
//! zero variance instead of a division by zero.
use serde::{Deserialize, Serialize};
use serde_json::Map;
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManagementReportingRequest {
    pub company_id: String,
    #[serde(default)]
    pub period: String,
    #[serde(default)]
    pub actuals: Map<String, serde_json::Value>,
    #[serde(default)]
    pub budget: Map<String, serde_json::Value>,
}

impl ManagementReportingRequest {
    fn actual(&self, key: &str) -> f64 {
        self.actuals
            .get(key)
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    }

    fn budgeted(&self, key: &str, fallback: f64) -> f64 {
        self.budget
            .get(key)
            .and_then(|v| v.as_f64())
            .unwrap_or(fallback)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VarianceLine {
    pub line_item: String,
    pub actual: f64,
    pub budget: f64,
    pub variance: f64,
    pub variance_pct: f64,
    pub status: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub total_revenue: f64,
    pub total_costs: f64,
    pub gross_margin: f64,
    pub ebitda: f64,
    pub net_margin: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ManagementKpis {
    pub revenue_variance: f64,
    pub cost_variance: f64,
    /// Fixed at 92.5 by the service; nothing behind it is measured.
    pub forecast_accuracy: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManagementReport {
    pub company_id: String,
    pub report_date: String,
    pub period: String,
    pub variance_analysis: Vec<VarianceLine>,
    pub performance_summary: PerformanceSummary,
    pub kpis: ManagementKpis,
    pub recommendations: Vec<String>,
}

/// `/generate`. `report_date` is supplied by the caller so the report is
/// reproducible; the service stamps the wall clock.
pub fn generate_management_report(
    req: &ManagementReportingRequest,
    report_date: &str,
) -> ManagementReport {
    let mut variance_analysis = Vec::with_capacity(req.actuals.len());
    for key in req.actuals.keys() {
        let actual = req.actual(key);
        let budget = req.budgeted(key, 0.0);
        let variance = actual - budget;
        let variance_pct = if budget != 0.0 {
            variance / budget * 100.0
        } else {
            0.0
        };
        variance_analysis.push(VarianceLine {
            line_item: key.clone(),
            actual: py_round2(actual),
            budget: py_round2(budget),
            variance: py_round2(variance),
            variance_pct: py_round2(variance_pct),
            status: if variance > 0.0 {
                "Favorable".to_string()
            } else {
                "Unfavorable".to_string()
            },
        });
    }

    let total_revenue = req.actual("revenue");
    let cogs = req.actual("cogs");
    let total_costs = cogs + req.actual("opex");

    let performance_summary = PerformanceSummary {
        total_revenue: py_round2(total_revenue),
        total_costs: py_round2(total_costs),
        gross_margin: if total_revenue != 0.0 {
            py_round2((total_revenue - cogs) / total_revenue * 100.0)
        } else {
            0.0
        },
        ebitda: py_round2(total_revenue - total_costs),
        net_margin: if total_revenue != 0.0 {
            py_round2(req.actual("net_income") / total_revenue * 100.0)
        } else {
            0.0
        },
    };

    // both budgets fall back to one, so an absent budget reads as zero variance
    let budget_revenue = req.budgeted("revenue", 1.0);
    let budget_costs = req.budgeted("cogs", 1.0) + req.budgeted("opex", 1.0);
    let kpis = ManagementKpis {
        revenue_variance: py_round2((total_revenue - budget_revenue) / budget_revenue * 100.0),
        cost_variance: py_round2((total_costs - budget_costs) / budget_costs * 100.0),
        forecast_accuracy: 92.5,
    };

    let over: Vec<&VarianceLine> = variance_analysis
        .iter()
        .filter(|v| v.status == "Unfavorable" && v.variance_pct.abs() > 5.0)
        .collect();
    let recommendations = if over.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "{} items significantly over budget - review drivers",
            over.len()
        )]
    };

    ManagementReport {
        company_id: req.company_id.clone(),
        report_date: report_date.to_string(),
        period: req.period.clone(),
        variance_analysis,
        performance_summary,
        kpis,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> ManagementReportingRequest {
        serde_json::from_value(json!({
            "company_id": "c1",
            "period": "2026-10",
            "actuals": {"revenue": 1000000, "cogs": 600000, "opex": 250000, "net_income": 90000},
            "budget": {"revenue": 950000, "cogs": 620000, "opex": 240000}
        }))
        .unwrap()
    }

    #[test]
    fn the_lines_follow_the_callers_order() {
        let report = generate_management_report(&request(), "2026-11-02T00:00:00");
        let names: Vec<&str> = report
            .variance_analysis
            .iter()
            .map(|v| v.line_item.as_str())
            .collect();
        assert_eq!(names, vec!["revenue", "cogs", "opex", "net_income"]);
        assert_eq!(report.variance_analysis[0].status, "Favorable");
        assert_eq!(report.variance_analysis[1].status, "Unfavorable");
    }

    #[test]
    fn the_summary_reads_the_headline_lines() {
        let summary = generate_management_report(&request(), "x").performance_summary;
        assert_eq!(summary.total_revenue, 1_000_000.0);
        assert_eq!(summary.total_costs, 850_000.0);
        assert_eq!(summary.gross_margin, 40.0);
        assert_eq!(summary.ebitda, 150_000.0);
        assert_eq!(summary.net_margin, 9.0);
    }

    #[test]
    fn an_absent_budget_reads_as_zero_variance() {
        let req = ManagementReportingRequest {
            company_id: "c1".to_string(),
            period: "2026-10".to_string(),
            actuals: serde_json::from_value(json!({"revenue": 500})).unwrap(),
            budget: Map::new(),
        };
        let report = generate_management_report(&req, "x");
        // the line's budget default is 0 ...
        assert_eq!(report.variance_analysis[0].budget, 0.0);
        assert_eq!(report.variance_analysis[0].variance_pct, 0.0);
        // ... but the KPI default is 1, so revenue variance is 49900%
        assert_eq!(report.kpis.revenue_variance, 49_900.0);
    }

    #[test]
    fn only_shortfalls_over_five_percent_earn_a_recommendation() {
        // every line here is at or above budget, so nothing is flagged
        let report = generate_management_report(&request(), "x");
        assert!(report.recommendations.is_empty());
        let short = generate_management_report(
            &ManagementReportingRequest {
                company_id: "c1".to_string(),
                actuals: serde_json::from_value(json!({"revenue": 800000})).unwrap(),
                budget: serde_json::from_value(json!({"revenue": 950000})).unwrap(),
                ..Default::default()
            },
            "x",
        );
        assert_eq!(
            short.recommendations,
            vec!["1 items significantly over budget - review drivers".to_string()]
        );
    }

    #[test]
    fn spending_over_budget_reads_as_favourable() {
        // the test is actual minus budget, so an overspent cost line passes it
        let report = generate_management_report(&request(), "x");
        let opex = report
            .variance_analysis
            .iter()
            .find(|v| v.line_item == "opex")
            .unwrap();
        assert_eq!(opex.variance, 10_000.0);
        assert_eq!(opex.status, "Favorable");
    }

    #[test]
    fn nothing_above_budget_is_unfavourable() {
        // a variance of exactly zero counts as unfavourable, as in the service
        let report = generate_management_report(
            &ManagementReportingRequest {
                company_id: "c1".to_string(),
                actuals: serde_json::from_value(json!({"revenue": 100})).unwrap(),
                budget: serde_json::from_value(json!({"revenue": 100})).unwrap(),
                ..Default::default()
            },
            "x",
        );
        assert_eq!(report.variance_analysis[0].status, "Unfavorable");
        assert_eq!(report.kpis.forecast_accuracy, 92.5);
    }

    #[test]
    fn an_empty_book_does_not_divide() {
        let report = generate_management_report(&ManagementReportingRequest::default(), "x");
        assert_eq!(report.performance_summary.gross_margin, 0.0);
        assert_eq!(report.performance_summary.net_margin, 0.0);
        assert!(report.variance_analysis.is_empty());
    }
}
