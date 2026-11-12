#![forbid(unsafe_code)]
//! Report preparation: the checklist arithmetic behind an annual report and
//! the filing readiness it implies. No money moves here; the crate only reads
//! what the books say is done.

pub mod analysis;
pub mod annual;
pub mod documents;
pub mod esg;
pub mod financial;
pub mod management;
pub mod project_accounting;
pub mod regulatory;
pub mod xbrl;

pub use analysis::{
    analyze_bank_fees, analyze_coverage, kpi_tracking_from_json, track_kpis, BankCharge,
    CovenantCompliance, CoverageMetrics, FeeAnalysisRequest, FeeAnalysisResult, FeeCharge, Kpi,
    KpiAnalysis, KpiSummary, KpiTrackingRequest, KpiTrackingResult, KpiTrend, TieRequest,
    TieResult,
};
pub use annual::{
    prepare_annual_report, AnnualReport, AnnualReportRequest, ReportSection, SectionInput,
};
pub use financial::{
    generate_financial_report, FinancialReport, FinancialReportingRequest, KeyMetrics,
};
pub use management::{
    generate_management_report, ManagementKpis, ManagementReport, ManagementReportingRequest,
    PerformanceSummary,
};
pub use regulatory::{generate_report, ReportRequest, ReportResult, ValidationCheck};

/// `business-documents-service /generate`.
pub fn business_documents_generate_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    documents::generate(payload)
}

/// `business-documents-service /generate/batch`.
pub fn business_documents_generate_batch_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    documents::generate_batch(payload)
}

/// `project-accounting-service /costs`.
pub fn project_costs_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    project_accounting::costs(payload)
}

/// `project-accounting-service /budget`.
pub fn project_budget_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    project_accounting::budget(payload)
}

/// `project-accounting-service /wip`.
pub fn project_wip_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    project_accounting::wip(payload)
}

/// `esg-reporting-service /report`.
pub fn esg_report_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    esg::report(payload)
}

/// `xbrl-reporting-service /generate`.
pub fn xbrl_generate_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    xbrl::generate(payload)
}
