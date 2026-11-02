//! comparative-financial-statements-service: generated multi-period
//! statements, horizontal/trend/variance analysis.
//!
//! PYTHON QUIRK: periods are simulated from a fixed 10m base with a linear 5%
//! step per period, not read from stored statements, so period five sits 20%
//! above the base rather than the 21.55% a compounded step would reach.
//!
//! The analyze block and the standalone horizontal endpoint publish different
//! shapes for the same arithmetic, so each has its own type here.
use serde::{Deserialize, Serialize};

const BASE_REVENUE: f64 = 10_000_000.0;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeriodData {
    pub period: String,
    pub revenue: f64,
    pub cost_of_sales: f64,
    pub gross_profit: f64,
    pub operating_expenses: f64,
    pub operating_profit: f64,
    pub net_profit: f64,
    pub total_assets: f64,
    pub total_liabilities: f64,
    pub total_equity: f64,
    pub cash_from_operations: f64,
}

pub fn generate_periods(number_of_periods: usize) -> Vec<PeriodData> {
    (0..number_of_periods)
        .map(|i| {
            let g = BASE_REVENUE * (1.0 + i as f64 * 0.05);
            PeriodData {
                period: format!("Period {}", i + 1),
                revenue: g,
                cost_of_sales: g * 0.55,
                gross_profit: g * 0.45,
                operating_expenses: g * 0.20,
                operating_profit: g * 0.25,
                net_profit: g * 0.18,
                total_assets: g * 4.0,
                total_liabilities: g * 2.2,
                total_equity: g * 1.8,
                cash_from_operations: g * 0.22,
            }
        })
        .collect()
}

fn pct_change(current: f64, prior: f64) -> f64 {
    if prior == 0.0 {
        0.0
    } else {
        (current - prior) / prior * 100.0
    }
}

/// One line of the analyze block's horizontal comparison. There is no
/// interpretation field here; only the standalone endpoint reads one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HorizontalItem {
    pub item_name: String,
    pub current_period: f64,
    pub prior_period: f64,
    pub change: f64,
    pub change_percentage: f64,
    pub absolute_change: f64,
}

fn horizontal_item(item_name: &str, current: f64, prior: f64) -> HorizontalItem {
    let change = current - prior;
    HorizontalItem {
        item_name: item_name.to_string(),
        current_period: current,
        prior_period: prior,
        change,
        change_percentage: pct_change(current, prior),
        absolute_change: change.abs(),
    }
}

/// The ten line items the analyze block compares period over period.
pub fn horizontal_items(periods: &[PeriodData]) -> Vec<HorizontalItem> {
    let n = periods.len();
    if n < 2 {
        return Vec::new();
    }
    let (cur, prior) = (&periods[n - 1], &periods[n - 2]);
    vec![
        horizontal_item("Revenue", cur.revenue, prior.revenue),
        horizontal_item("Cost of Sales", cur.cost_of_sales, prior.cost_of_sales),
        horizontal_item("Gross Profit", cur.gross_profit, prior.gross_profit),
        horizontal_item(
            "Operating Expenses",
            cur.operating_expenses,
            prior.operating_expenses,
        ),
        horizontal_item(
            "Operating Profit",
            cur.operating_profit,
            prior.operating_profit,
        ),
        horizontal_item("Net Profit", cur.net_profit, prior.net_profit),
        horizontal_item("Total Assets", cur.total_assets, prior.total_assets),
        horizontal_item(
            "Total Liabilities",
            cur.total_liabilities,
            prior.total_liabilities,
        ),
        horizontal_item("Total Equity", cur.total_equity, prior.total_equity),
        horizontal_item(
            "Cash from Operations",
            cur.cash_from_operations,
            prior.cash_from_operations,
        ),
    ]
}

/// `/horizontal-analysis`: a single item, with its own field names
/// (`percentage_change`, not `change_percentage`) and a plain verdict.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HorizontalChange {
    pub item_name: String,
    pub current_period: f64,
    pub prior_period: f64,
    pub absolute_change: f64,
    pub percentage_change: f64,
    pub interpretation: String,
}

pub fn horizontal_analysis(
    current_value: f64,
    prior_value: f64,
    item_name: &str,
) -> HorizontalChange {
    let change = current_value - prior_value;
    let interpretation = if change > 0.0 {
        "Favorable"
    } else if change < 0.0 {
        "Unfavorable"
    } else {
        "No change"
    };
    HorizontalChange {
        item_name: item_name.to_string(),
        current_period: current_value,
        prior_period: prior_value,
        absolute_change: change,
        percentage_change: pct_change(current_value, prior_value),
        interpretation: interpretation.to_string(),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrendPoint {
    pub period: String,
    pub value: f64,
}

/// One item's trend inside the analyze block.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrendItem {
    pub item_name: String,
    pub base_period_value: f64,
    pub periods: Vec<TrendPoint>,
    pub trend_percentages: Vec<f64>,
    pub compound_annual_growth_rate: f64,
}

fn pick(period: &PeriodData, item_name: &str) -> f64 {
    match item_name {
        "Revenue" => period.revenue,
        "Net Profit" => period.net_profit,
        "Total Assets" => period.total_assets,
        _ => period.operating_profit,
    }
}

/// Trend plus CAGR for one item. CAGR is 0 when the base is not positive; the
/// number of compounding steps is `len - 1`.
pub fn trend_analysis(periods: &[PeriodData], item_name: &str) -> TrendItem {
    let base_value = periods.first().map(|p| pick(p, item_name)).unwrap_or(0.0);
    let n_periods = periods.len().saturating_sub(1) as f64;
    let cagr = if base_value > 0.0 && n_periods > 0.0 {
        let final_value = periods.last().map(|p| pick(p, item_name)).unwrap_or(0.0);
        ((final_value / base_value).powf(1.0 / n_periods) - 1.0) * 100.0
    } else {
        0.0
    };
    TrendItem {
        item_name: item_name.to_string(),
        base_period_value: base_value,
        periods: periods
            .iter()
            .map(|p| TrendPoint {
                period: p.period.clone(),
                value: pick(p, item_name),
            })
            .collect(),
        trend_percentages: periods
            .iter()
            .map(|p| {
                if base_value == 0.0 {
                    0.0
                } else {
                    pick(p, item_name) / base_value * 100.0
                }
            })
            .collect(),
        compound_annual_growth_rate: cagr,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrendIndex {
    pub period: String,
    pub value: f64,
    pub trend: f64,
}

/// `/trend-analysis`: a series of any values, indexed to its first period.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrendSeries {
    pub base_period: String,
    pub base_value: f64,
    pub final_period: String,
    pub final_value: f64,
    pub trend_percentages: Vec<TrendIndex>,
    pub compound_annual_growth_rate: f64,
    pub average_growth: f64,
}

/// The service rejects a series shorter than two periods with a 400, so
/// `None` here is the caller's cue to answer that.
pub fn trend_series(values: &[f64], periods: &[String]) -> Option<TrendSeries> {
    if values.len() < 2 || periods.len() < 2 {
        return None;
    }
    let base_value = values[0];
    let span = (values.len() - 1) as f64;
    let cagr = if base_value > 0.0 {
        ((values[values.len() - 1] / base_value).powf(1.0 / span) - 1.0) * 100.0
    } else {
        0.0
    };
    let growth: f64 = (1..values.len())
        .map(|i| (values[i] - values[i - 1]) / values[i - 1])
        .sum();
    Some(TrendSeries {
        base_period: periods[0].clone(),
        base_value,
        final_period: periods[periods.len() - 1].clone(),
        final_value: values[values.len() - 1],
        trend_percentages: periods
            .iter()
            .zip(values)
            .map(|(period, value)| TrendIndex {
                period: period.clone(),
                value: *value,
                trend: value / base_value * 100.0,
            })
            .collect(),
        compound_annual_growth_rate: cagr,
        average_growth: growth / span * 100.0,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VarianceLine {
    pub item: String,
    pub budgeted: f64,
    pub actual: f64,
    pub variance: f64,
    pub variance_percentage: f64,
    pub favorable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VarianceReport {
    pub variances: Vec<VarianceLine>,
    pub total_favorable_variance: f64,
    pub total_unfavorable_variance: f64,
}

/// `/variance-analysis`. Only the first `min(len)` indexes are compared,
/// and a zero variance counts as unfavourable (Python's `> 0` test).
pub fn variance_analysis(budgeted: &[f64], actual: &[f64], items: &[String]) -> VarianceReport {
    let mut variances = Vec::new();
    for (i, item) in items.iter().enumerate() {
        if i < budgeted.len() && i < actual.len() {
            let variance = actual[i] - budgeted[i];
            variances.push(VarianceLine {
                item: item.clone(),
                budgeted: budgeted[i],
                actual: actual[i],
                variance,
                variance_percentage: pct_change(actual[i], budgeted[i]),
                favorable: variance > 0.0,
            });
        }
    }
    let total_favorable_variance = variances
        .iter()
        .filter(|v| v.favorable)
        .map(|v| v.variance)
        .sum();
    let total_unfavorable_variance = variances
        .iter()
        .filter(|v| !v.favorable)
        .map(|v| v.variance.abs())
        .sum();
    VarianceReport {
        variances,
        total_favorable_variance,
        total_unfavorable_variance,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RatioComparison {
    pub ratio_name: String,
    pub current_value: f64,
    pub prior_value: f64,
    pub change: f64,
    pub industry_benchmark: f64,
}

/// The four benchmarks the service compares against, unchanged.
fn ratio_comparison(periods: &[PeriodData]) -> Vec<RatioComparison> {
    let n = periods.len();
    let (cur, prior) = (&periods[n - 1], &periods[n.saturating_sub(2)]);
    let margin = |part: f64, whole: f64| part / whole * 100.0;
    let line = |name: &str, read: fn(&PeriodData) -> f64, whole: fn(&PeriodData) -> f64, bench| {
        let current_value = margin(read(cur), whole(cur));
        let prior_value = margin(read(prior), whole(prior));
        RatioComparison {
            ratio_name: name.to_string(),
            change: current_value - prior_value,
            current_value,
            prior_value,
            industry_benchmark: bench,
        }
    };
    vec![
        line("Gross Margin", |p| p.gross_profit, |p| p.revenue, 45.0),
        line(
            "Operating Margin",
            |p| p.operating_profit,
            |p| p.revenue,
            20.0,
        ),
        line(
            "Return on Assets",
            |p| p.net_profit,
            |p| p.total_assets,
            8.0,
        ),
        line(
            "Debt Ratio",
            |p| p.total_liabilities,
            |p| p.total_assets,
            55.0,
        ),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparativeRequest {
    pub company_id: String,
    pub current_period: String,
    pub prior_period: String,
    #[serde(default)]
    pub base_period: Option<String>,
    #[serde(default = "yes")]
    pub include_trend: bool,
    #[serde(default = "yes")]
    pub include_ratio_comparison: bool,
    /// The service accepts three to ten periods; the binding checks it.
    #[serde(default = "five")]
    pub number_of_periods: usize,
}

fn yes() -> bool {
    true
}

fn five() -> usize {
    5
}

impl Default for ComparativeRequest {
    fn default() -> Self {
        Self {
            company_id: String::new(),
            current_period: String::new(),
            prior_period: String::new(),
            base_period: None,
            include_trend: true,
            include_ratio_comparison: true,
            number_of_periods: five(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComparativeAnalysis {
    pub company_id: String,
    pub periods: Vec<String>,
    pub horizontal_analysis: Vec<HorizontalItem>,
    pub trend_analysis: Option<Vec<TrendItem>>,
    pub ratio_comparison: Option<Vec<RatioComparison>>,
    pub summary_statistics: SummaryStatistics,
}

/// The four headline figures, in the order the service builds them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SummaryStatistics {
    pub average_revenue_growth: f64,
    pub average_profit_margin: f64,
    pub total_asset_growth: f64,
    pub equity_compound_growth: f64,
}

/// `/analyze`. The period count is clamped here; the three-to-ten rule is the
/// endpoint's job.
pub fn analyze(req: &ComparativeRequest) -> ComparativeAnalysis {
    let periods = generate_periods(req.number_of_periods.clamp(1, 10));
    let span = periods.len().saturating_sub(1).max(1) as f64;

    let trend_analysis = if req.include_trend {
        Some(
            ["Revenue", "Net Profit", "Total Assets", "Operating Profit"]
                .into_iter()
                .map(|name| trend_analysis(&periods, name))
                .collect(),
        )
    } else {
        None
    };

    let ratio_comparison = if req.include_ratio_comparison && periods.len() >= 2 {
        Some(ratio_comparison(&periods))
    } else {
        None
    };

    let growth: f64 = (1..periods.len())
        .map(|i| (periods[i].revenue - periods[i - 1].revenue) / periods[i - 1].revenue)
        .sum();
    let margin_sum: f64 = periods.iter().map(|p| p.net_profit / p.revenue).sum();
    let summary_statistics = SummaryStatistics {
        average_revenue_growth: growth / span * 100.0,
        average_profit_margin: margin_sum / periods.len() as f64 * 100.0,
        total_asset_growth: (periods[periods.len() - 1].total_assets - periods[0].total_assets)
            / periods[0].total_assets
            * 100.0,
        equity_compound_growth: ((periods[periods.len() - 1].total_equity
            / periods[0].total_equity)
            .powf(1.0 / span)
            - 1.0)
            * 100.0,
    };

    ComparativeAnalysis {
        company_id: req.company_id.clone(),
        periods: periods.iter().map(|p| p.period.clone()).collect(),
        horizontal_analysis: horizontal_items(&periods),
        trend_analysis,
        ratio_comparison,
        summary_statistics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ComparativeRequest {
        ComparativeRequest {
            company_id: "c1".to_string(),
            current_period: "2026".to_string(),
            prior_period: "2025".to_string(),
            number_of_periods: 5,
            ..Default::default()
        }
    }

    #[test]
    fn periods_step_five_percent_linearly() {
        let p = generate_periods(3);
        assert_eq!(p[0].revenue, 10_000_000.0);
        assert_eq!(p[1].revenue, 10_500_000.0);
        assert_eq!(p[2].revenue, 11_000_000.0);
        // float accumulation matches Python: 10m * 1.1 is not exact
        assert!((p[2].cost_of_sales - 6_050_000.0).abs() < 1e-6);
        assert!((p[2].total_assets - 44_000_000.0).abs() < 1e-6);
        assert_eq!(p[0].period, "Period 1");
    }

    #[test]
    fn horizontal_compares_the_last_two_periods() {
        let items = horizontal_items(&generate_periods(3));
        assert_eq!(items.len(), 10);
        assert_eq!(items[0].item_name, "Revenue");
        assert_eq!(items[9].item_name, "Cash from Operations");
        assert!((items[0].change - 500_000.0).abs() < 1e-6);
        assert!((items[0].change_percentage - 4.761_904_761_904_762).abs() < 1e-9);
        assert!((items[0].absolute_change - 500_000.0).abs() < 1e-6);
    }

    #[test]
    fn the_standalone_endpoint_keeps_its_own_field_names() {
        let h = horizontal_analysis(500.0, 400.0, "Rent");
        assert_eq!(h.absolute_change, 100.0);
        assert!((h.percentage_change - 25.0).abs() < 1e-9);
        assert_eq!(h.interpretation, "Favorable");
        let flat = horizontal_analysis(500.0, 500.0, "Rent");
        assert_eq!(flat.interpretation, "No change");
        let zero = horizontal_analysis(500.0, 0.0, "Rent");
        assert_eq!(zero.percentage_change, 0.0);
        assert_eq!(
            horizontal_analysis(400.0, 500.0, "Rent").interpretation,
            "Unfavorable"
        );
    }

    #[test]
    fn the_trend_indexes_each_period_to_the_base() {
        let series = trend_series(
            &[100.0, 110.0, 121.0],
            &["2023".to_string(), "2024".to_string(), "2025".to_string()],
        )
        .unwrap();
        assert_eq!(series.base_period, "2023");
        assert_eq!(series.final_period, "2025");
        assert_eq!(series.trend_percentages[2].trend, 121.0);
        assert!((series.compound_annual_growth_rate - 10.0).abs() < 1e-9);
        assert!((series.average_growth - 10.0).abs() < 1e-9);
        assert!(trend_series(&[100.0], &["2026".to_string()]).is_none());
    }

    #[test]
    fn the_analyze_trend_runs_at_five_percent_a_step() {
        let trends = analyze(&request()).trend_analysis.unwrap();
        assert_eq!(trends.len(), 4);
        let revenue = trends.iter().find(|t| t.item_name == "Revenue").unwrap();
        assert_eq!(revenue.base_period_value, 10_000_000.0);
        assert!((revenue.trend_percentages[4] - 120.0).abs() < 1e-9);
        let expected = (1.2f64.powf(0.25) - 1.0) * 100.0;
        assert!((revenue.compound_annual_growth_rate - expected).abs() < 1e-9);
    }

    #[test]
    fn the_margins_are_flat_across_the_series() {
        let ratios = analyze(&request()).ratio_comparison.unwrap();
        assert_eq!(ratios.len(), 4);
        assert!((ratios[0].current_value - 45.0).abs() < 1e-9);
        assert!(ratios[0].change.abs() < 1e-9);
        assert_eq!(ratios[0].industry_benchmark, 45.0);
        assert!((ratios[3].current_value - 55.0).abs() < 1e-9);
        assert_eq!(ratios[3].industry_benchmark, 55.0);
    }

    #[test]
    fn the_summary_reports_the_headline_growth() {
        let summary = analyze(&request()).summary_statistics;
        // the step shrinks as the base grows, so the average sits under 5%
        let expected_growth = (1..5)
            .map(|i| 0.05 / (1.0 + (i - 1) as f64 * 0.05))
            .sum::<f64>()
            / 4.0
            * 100.0;
        assert!((summary.average_revenue_growth - expected_growth).abs() < 1e-9);
        assert!(summary.average_revenue_growth < 5.0);
        assert!((summary.average_profit_margin - 18.0).abs() < 1e-9);
        assert!((summary.total_asset_growth - 20.0).abs() < 1e-9);
        let equity = (1.2f64.powf(0.25) - 1.0) * 100.0;
        assert!((summary.equity_compound_growth - equity).abs() < 1e-9);
    }

    #[test]
    fn the_optional_blocks_can_be_left_out() {
        let analysis = analyze(&ComparativeRequest {
            include_trend: false,
            include_ratio_comparison: false,
            ..request()
        });
        assert!(analysis.trend_analysis.is_none());
        assert!(analysis.ratio_comparison.is_none());
        assert_eq!(analysis.horizontal_analysis.len(), 10);
    }

    #[test]
    fn variances_separate_favourable_from_unfavourable() {
        let report = variance_analysis(
            &[100.0, 200.0, 50.0],
            &[120.0, 180.0, 50.0],
            &["Sales".to_string(), "Costs".to_string(), "Rent".to_string()],
        );
        assert_eq!(report.total_favorable_variance, 20.0);
        assert_eq!(report.total_unfavorable_variance, 20.0);
        assert_eq!(report.variances[1].variance_percentage, -10.0);
        // equal to budget is not favourable
        assert!(!report.variances[2].favorable);
    }

    #[test]
    fn items_past_the_end_of_a_series_are_skipped() {
        let report = variance_analysis(&[100.0], &[90.0], &["A".to_string(), "B".to_string()]);
        assert_eq!(report.variances.len(), 1);
        assert_eq!(report.total_unfavorable_variance, 10.0);
    }
}
