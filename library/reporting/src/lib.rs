#![forbid(unsafe_code)]
//! Report preparation: the checklist arithmetic behind an annual report and
//! the filing readiness it implies. No money moves here; the crate only reads
//! what the books say is done.

pub mod annual;
pub mod financial;
pub mod management;

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
