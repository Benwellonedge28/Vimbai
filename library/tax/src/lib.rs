#![forbid(unsafe_code)]
//! Tax math: progressive/tax-type calculation, capital gains, corporate
//! tax provision (deferred tax), group consolidation, R&D credits,
//! transfer pricing and tax risk assessment.
//!
//! Ported from: tax-calculation-service, tax-service,
//! tax-provision-service, group-tax-service, r-and-d-tax-service,
//! tax-return-preparation-service and tax-risk-service.
//!
//! Pure computation only. Vimbai never moves money; these are
//! record-keeping calculations.

pub mod capital_gains;
pub mod corporate;
pub mod group;
pub mod rd_credit;
pub mod returns;
pub mod risk;
pub mod statutory;
pub mod transfer_pricing;

pub use capital_gains::{capital_gains_tax, CapitalGainsResult};
pub use corporate::{
    calculate_progressive_tax, calculate_tax, default_zw_brackets, tax_provision, TaxBracket,
    TaxCalculation, TaxProvisionRequest, TaxProvisionResult, TaxType, TempDifference,
    TempDifferenceType,
};
pub use group::{consolidate_group_tax, GroupTaxRequest, GroupTaxResult, SubsidiaryTax};
pub use rd_credit::{rd_credit_claim, Expenditure, RdClaimResult};
pub use returns::{prepare_tax_return, TaxComponent, TaxReturnRequest, TaxReturnResult};
pub use risk::{assess_tax_risk, RiskLevel, TaxRisk, TaxRiskAssessment, TaxRiskRequest};
pub use statutory::{
    statutory_corporate_tax, CorporateTaxInput, CorporateTaxResult, Installment, TaxAdjustment,
    TaxCredit,
};
pub use transfer_pricing::{transfer_pricing, TransferPricingResult};
