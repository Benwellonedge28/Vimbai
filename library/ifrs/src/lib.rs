#![forbid(unsafe_code)]
//! IFRS and standards compliance math: IFRS 15/16/2/9, revenue recognition,
//! government grants, construction contracts, segment reporting, going
//! concern, XBRL mappings
//!
//! Ported so far: IFRS 2 share-based payment (see `share_based`), IFRS 9
//! instruments (see `instruments`), IFRS 15 revenue (see `revenue`) and
//! IFRS 16 leases (see `lease_accounting`).

pub mod instruments;
pub mod lease_accounting;
pub mod revenue;
pub mod share_based;

pub use lease_accounting::{
    assess_renewal_options, calculate_ifrs16, calculate_sale_leaseback,
    calculate_variable_payments, measure_lease_liability, AmortizationRow, Ifrs16Request,
    Ifrs16Response, LeaseDetails, LeaseError, LeaseExpenseBreakdown, LeaseLiabilityMeasurement,
    LeaseLiabilitySchedule, RenewalAssessment, RouAssetMeasurement, SaleLeaseback, VariablePayment,
};

pub use revenue::{
    account_for_modification, assess_customer_options, calculate_ssp_allocation,
    estimate_variable_consideration, measure_progress, recognize_revenue, ContractAssetLiability,
    ContractDetails, ContractModification, CustomerOption, Ifrs15Response, PerformanceObligation,
    ProgressMeasurement, RevenueRecognitionRequest, SspAllocation, Step1Identification,
    Step2Obligations, Step3PriceAllocation, Step4Satisfaction, VariableConsideration,
};

pub use share_based::{
    account_for_grant_modification, account_for_group_transaction, assess_performance_condition,
    calculate_instrument_fair_value, calculate_share_based_payment, GrantModification,
    GroupTransaction, Ifrs2Response, InstrumentFairValue, PerformanceCondition, ShareBasedError,
    ShareBasedPaymentRequest, ShareOption, VestingCalculation, VestingCondition,
};

pub use instruments::{
    assess_business_model, assess_hedge_effectiveness, calculate_ecl_general,
    calculate_ecl_simplified, classify_and_measure, perform_sppi_test, AgeBucketEcl,
    BusinessModelAssessment, ClassificationResult, EclCalculation, FinancialAsset, GeneralEcl,
    HedgeEffectiveness, Ifrs9Request, Ifrs9Response, MeasurementResult, SimplifiedEcl, SppiResult,
};
