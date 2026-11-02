//! IFRS 8 operating segments: identifying reportable segments and the
//! reconciliation back to the consolidated statements.
//!
//! A segment is reportable when any one of its revenue, absolute profit or
//! assets reaches a tenth of the total, which is the IFRS 8 threshold checked
//! against all three measures. The service keeps the threshold tests exactly
//! as they read, including the two that divide by a total which may be zero.
use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, py_round2};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Segment {
    #[serde(default)]
    pub segment_id: String,
    pub name: String,
    /// `business` or `geographical`. The request calls it `segment_type` and
    /// the report emits it as `type`.
    #[serde(rename = "segment_type", alias = "type")]
    pub segment_type: String,
    pub revenue: f64,
    pub expenses: f64,
    pub assets: f64,
    #[serde(default)]
    pub liabilities: f64,
    /// The service's field name is `profit_or_loss`.
    #[serde(default, rename = "profit_or_loss")]
    pub profit_or_loss: f64,
    #[serde(default = "default_true")]
    pub identifiable: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SegmentReportRequest {
    pub company_id: String,
    pub fiscal_year: i64,
    pub segments: Vec<Segment>,
    #[serde(default)]
    pub reconciliation_revenue: f64,
    #[serde(default)]
    pub reconciliation_expenses: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SegmentDetail {
    pub segment_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub segment_type: String,
    pub revenue: f64,
    pub expenses: f64,
    pub profit_or_loss: f64,
    pub assets: f64,
    pub liabilities: f64,
    pub revenue_pct: f64,
    pub is_reportable: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Reconciliation {
    pub segments_revenue: f64,
    pub consolidated_revenue: f64,
    pub reconciliation_difference: f64,
    pub segments_expenses: f64,
    pub consolidated_expenses: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SegmentReport {
    pub company_id: String,
    pub fiscal_year: i64,
    pub reportable_segments: i64,
    pub total_revenue: f64,
    pub total_profit: f64,
    pub total_assets: f64,
    pub segments: Vec<SegmentDetail>,
    pub reconciliation: Reconciliation,
    pub disclosure_notes: Vec<String>,
}

/// The percentage a segment represents, or zero when the total is zero. The
/// profit measure uses the absolute totals on both sides, so a loss-making
/// segment is measured on its size rather than its sign.
fn pct(part: f64, total: f64) -> f64 {
    if total == 0.0 {
        0.0
    } else {
        part / total * 100.0
    }
}

pub fn report_segments(req: &SegmentReportRequest) -> SegmentReport {
    let total_revenue: f64 = req.segments.iter().map(|s| s.revenue).sum();
    let total_profit: f64 = req.segments.iter().map(|s| s.profit_or_loss).sum();
    let total_assets: f64 = req.segments.iter().map(|s| s.assets).sum();
    let total_expenses: f64 = req.segments.iter().map(|s| s.expenses).sum();

    let mut reportable = 0;
    let mut details = Vec::with_capacity(req.segments.len());
    for segment in &req.segments {
        let revenue_pct = pct(segment.revenue, total_revenue);
        let profit_pct = pct(segment.profit_or_loss.abs(), total_profit.abs());
        let asset_pct = pct(segment.assets, total_assets);
        let is_reportable = revenue_pct >= 10.0 || profit_pct >= 10.0 || asset_pct >= 10.0;
        if is_reportable {
            reportable += 1;
        }
        details.push(SegmentDetail {
            segment_id: segment.segment_id.clone(),
            name: segment.name.clone(),
            segment_type: segment.segment_type.clone(),
            revenue: py_round2(segment.revenue),
            expenses: py_round2(segment.expenses),
            profit_or_loss: py_round2(segment.profit_or_loss),
            assets: py_round2(segment.assets),
            liabilities: py_round2(segment.liabilities),
            revenue_pct: py_round(revenue_pct, 1),
            is_reportable,
        });
    }

    SegmentReport {
        company_id: req.company_id.clone(),
        fiscal_year: req.fiscal_year,
        reportable_segments: reportable,
        total_revenue: py_round2(total_revenue),
        total_profit: py_round2(total_profit),
        total_assets: py_round2(total_assets),
        segments: details,
        reconciliation: Reconciliation {
            segments_revenue: py_round2(total_revenue),
            consolidated_revenue: py_round2(total_revenue + req.reconciliation_revenue),
            reconciliation_difference: py_round2(req.reconciliation_revenue),
            segments_expenses: py_round2(total_expenses),
            consolidated_expenses: py_round2(total_expenses + req.reconciliation_expenses),
        },
        disclosure_notes: vec![
            format!("{reportable} reportable segments identified under IFRS 8"),
            "Segment identification based on internal management reporting".to_string(),
            "Geographic and business segment information disclosed separately".to_string(),
            "Inter-segment transactions eliminated in reconciliation".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(id: &str, revenue: f64, profit: f64, assets: f64) -> Segment {
        Segment {
            segment_id: id.to_string(),
            name: id.to_string(),
            segment_type: "business".to_string(),
            revenue,
            expenses: revenue - profit,
            assets,
            liabilities: 0.0,
            profit_or_loss: profit,
            identifiable: true,
        }
    }

    fn request() -> SegmentReportRequest {
        SegmentReportRequest {
            company_id: "c1".to_string(),
            fiscal_year: 2026,
            segments: vec![
                segment("retail", 800_000.0, 120_000.0, 1_600_000.0),
                segment("wholesale", 150_000.0, 10_000.0, 300_000.0),
                // 5% of revenue, 5% of assets, 1.5% of profit: below every bar
                segment("other", 50_000.0, 2_000.0, 100_000.0),
            ],
            reconciliation_revenue: -25_000.0,
            reconciliation_expenses: 4_000.0,
        }
    }

    #[test]
    fn the_threshold_is_a_tenth_on_any_measure() {
        let report = report_segments(&request());
        assert_eq!(report.reportable_segments, 2);
        let small = report
            .segments
            .iter()
            .find(|s| s.segment_id == "other")
            .unwrap();
        assert!((small.revenue_pct - 5.0).abs() < 1e-9);
        assert!(!small.is_reportable);
    }

    #[test]
    fn reconciliation_carries_the_elimination_into_the_totals() {
        let report = report_segments(&request());
        assert_eq!(report.total_revenue, 1_000_000.0);
        assert_eq!(report.reconciliation.segments_revenue, 1_000_000.0);
        assert_eq!(report.reconciliation.consolidated_revenue, 975_000.0);
        assert_eq!(report.reconciliation.reconciliation_difference, -25_000.0);
        assert_eq!(report.reconciliation.segments_expenses, 868_000.0);
        assert_eq!(report.reconciliation.consolidated_expenses, 872_000.0);
    }

    #[test]
    fn an_empty_book_reports_nothing_rather_than_dividing_by_zero() {
        let report = report_segments(&SegmentReportRequest {
            company_id: "c1".to_string(),
            fiscal_year: 2026,
            ..Default::default()
        });
        assert_eq!(report.reportable_segments, 0);
        assert!(report.segments.is_empty());
        assert_eq!(report.total_revenue, 0.0);
    }

    #[test]
    fn the_disclosure_counts_what_qualifies() {
        let report = report_segments(&request());
        assert!(report.disclosure_notes[0].starts_with("2 reportable segments"));
    }
}
