//! comparative-financial-statements-service: generated multi-period
//! statements, horizontal/trend/variance analysis.
use serde::{Deserialize, Serialize};

/// PYTHON QUIRK: periods are simulated from a fixed 10m base with a
/// linear 5% step per period, not read from stored statements.
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HorizontalItem {
    pub item_name: String,
    pub current_period: f64,
    pub prior_period: f64,
    pub change: f64,
    pub change_percentage: f64,
    pub absolute_change: f64,
    pub interpretation: String,
}

fn pct_change(current: f64, prior: f64) -> f64 {
    if prior == 0.0 {
        0.0
    } else {
        (current - prior) / prior * 100.0
    }
}

fn interpretation(change: f64) -> &'static str {
    if change > 0.0 {
        "Favorable"
    } else if change < 0.0 {
        "Unfavorable"
    } else {
        "No change"
    }
}

/// `/horizontal-analysis` for a single item.
pub fn horizontal_analysis(
    current_value: f64,
    prior_value: f64,
    item_name: &str,
) -> HorizontalItem {
    let change = current_value - prior_value;
    HorizontalItem {
        item_name: item_name.to_string(),
        current_period: current_value,
        prior_period: prior_value,
        change,
        change_percentage: pct_change(current_value, prior_value),
        absolute_change: change.abs(),
        interpretation: interpretation(change).to_string(),
    }
}

/// The five line items the service compares period over period.
pub fn horizontal_items(periods: &[PeriodData]) -> Vec<HorizontalItem> {
    let n = periods.len();
    if n < 2 {
        return Vec::new();
    }
    let (cur, prior) = (&periods[n - 1], &periods[n - 2]);
    vec![
        horizontal_analysis(cur.revenue, prior.revenue, "Revenue"),
        horizontal_analysis(cur.cost_of_sales, prior.cost_of_sales, "Cost of Sales"),
        horizontal_analysis(cur.gross_profit, prior.gross_profit, "Gross Profit"),
        horizontal_analysis(
            cur.operating_profit,
            prior.operating_profit,
            "Operating Profit",
        ),
        horizontal_analysis(cur.net_profit, prior.net_profit, "Net Profit"),
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    pub period: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendItem {
    pub item_name: String,
    pub base_period_value: f64,
    pub periods: Vec<TrendPoint>,
    pub percentages: Vec<f64>,
    pub cagr: f64,
}

/// Trend plus CAGR. CAGR is 0 when the base is not positive; the number
/// of compounding steps is `len - 1`, so 2 periods divide the exponent by 1.
pub fn trend_analysis(periods: &[PeriodData], item_name: &str) -> TrendItem {
    let base_value = match item_name {
        "Revenue" => periods.first().map(|p| p.revenue),
        "Net Profit" => periods.first().map(|p| p.net_profit),
        "Total Assets" => periods.first().map(|p| p.total_assets),
        _ => periods.first().map(|p| p.operating_profit),
    }
    .unwrap_or(0.0);
    let pick = |p: &PeriodData| match item_name {
        "Revenue" => p.revenue,
        "Net Profit" => p.net_profit,
        "Total Assets" => p.total_assets,
        _ => p.operating_profit,
    };
    let percentages: Vec<f64> = periods
        .iter()
        .map(|p| {
            if base_value == 0.0 {
                0.0
            } else {
                pick(p) / base_value * 100.0
            }
        })
        .collect();
    let n_periods = periods.len().saturating_sub(1) as f64;
    let cagr = if base_value > 0.0 && n_periods > 0.0 {
        let final_value = periods.last().map(pick).unwrap_or(0.0);
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
                value: pick(p),
            })
            .collect(),
        percentages,
        cagr,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarianceLine {
    pub item: String,
    pub budgeted: f64,
    pub actual: f64,
    pub variance: f64,
    pub variance_percentage: f64,
    pub favorable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn horizontal_compares_last_two_periods() {
        let items = horizontal_items(&generate_periods(3));
        assert_eq!(items.len(), 5);
        assert_eq!(items[0].item_name, "Revenue");
        assert!((items[0].change - 500_000.0).abs() < 1e-6);
        assert!((items[0].change_percentage - 4.761_904_761_904_762).abs() < 1e-9);
        assert_eq!(items[0].interpretation, "Favorable");
    }

    #[test]
    fn flat_item_reads_no_change() {
        let h = horizontal_analysis(500.0, 500.0, "Rent");
        assert_eq!(h.change, 0.0);
        assert_eq!(h.interpretation, "No change");
        let z = horizontal_analysis(500.0, 0.0, "Rent");
        assert_eq!(z.change_percentage, 0.0);
    }

    #[test]
    fn trend_percentages_and_cagr() {
        let t = trend_analysis(&generate_periods(3), "Revenue");
        assert_eq!(t.base_period_value, 10_000_000.0);
        assert!((t.percentages[0] - 100.0).abs() < 1e-9);
        assert!((t.percentages[1] - 105.0).abs() < 1e-9);
        assert!((t.percentages[2] - 110.0).abs() < 1e-9);
        // (110/100)^(1/2) - 1 = 4.8809%
        assert!((t.cagr - 4.880_884_288_749_783).abs() < 1e-6, "{}", t.cagr);
        assert_eq!(t.periods.len(), 3);
    }

    #[test]
    fn cagr_guarded_when_base_not_positive() {
        let t = trend_analysis(&[], "Revenue");
        assert_eq!(t.cagr, 0.0);
        assert_eq!(t.base_period_value, 0.0);
    }

    #[test]
    fn variance_splits_favourable_and_not() {
        let r = variance_analysis(
            &[1_000.0, 2_000.0, 500.0],
            &[1_200.0, 1_800.0, 500.0],
            &["sales".into(), "costs".into(), "rent".into()],
        );
        assert_eq!(r.variances.len(), 3);
        assert!(r.variances[0].favorable);
        assert!(!r.variances[1].favorable);
        // a zero variance is counted unfavourable, like Python's `> 0`
        assert!(!r.variances[2].favorable);
        assert_eq!(r.total_favorable_variance, 200.0);
        assert_eq!(r.total_unfavorable_variance, 200.0);
    }

    #[test]
    fn variance_stops_at_shortest_input() {
        let r = variance_analysis(&[100.0], &[110.0, 120.0], &["a".into(), "b".into()]);
        assert_eq!(r.variances.len(), 1);
    }
}
