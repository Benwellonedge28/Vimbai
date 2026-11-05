#![forbid(unsafe_code)]
//! Report preparation: the checklist arithmetic behind an annual report and
//! the filing readiness it implies. No money moves here; the crate only reads
//! what the books say is done.

pub mod analysis;
pub mod annual;
pub mod documents;
pub mod financial;
pub mod management;
pub mod regulatory;

pub use analysis::{
    analyze_bank_fees, analyze_coverage, analyze_suspense, track_kpis, BankCharge, Correction,
    CovenantCompliance, CoverageMetrics, FeeAnalysisRequest, FeeAnalysisResult, FeeCharge, Kpi,
    KpiAnalysis, KpiSummary, KpiTrackingRequest, KpiTrackingResult, KpiTrend, SuspenseError,
    SuspenseRequest, SuspenseResult, TieRequest, TieResult,
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
