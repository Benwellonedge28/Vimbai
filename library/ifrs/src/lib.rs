#![forbid(unsafe_code)]
//! IFRS and standards compliance math: IFRS 15/16/2/9, revenue recognition,
//! government grants, construction contracts, segment reporting, going
//! concern, XBRL mappings
//!
//! Ported so far: IFRS 16 leases (see `lease_accounting`).

pub mod lease_accounting;

pub use lease_accounting::{
    assess_renewal_options, calculate_ifrs16, calculate_sale_leaseback,
    calculate_variable_payments, measure_lease_liability, AmortizationRow, Ifrs16Request,
    Ifrs16Response, LeaseDetails, LeaseError, LeaseExpenseBreakdown, LeaseLiabilityMeasurement,
    LeaseLiabilitySchedule, RenewalAssessment, RouAssetMeasurement, SaleLeaseback, VariablePayment,
};
