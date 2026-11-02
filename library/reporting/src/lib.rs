#![forbid(unsafe_code)]
//! Report preparation: the checklist arithmetic behind an annual report and
//! the filing readiness it implies. No money moves here; the crate only reads
//! what the books say is done.

pub mod annual;

pub use annual::{
    prepare_annual_report, AnnualReport, AnnualReportRequest, ReportSection, SectionInput,
};
