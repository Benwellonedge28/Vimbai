//! Vimbai tax calculations.
//!
//! Seven stateless services, one crate, no database and no clock: the caller
//! supplies the date where one is needed. Every quirk the services carried is
//! preserved and commented, because filed figures must not shift under a port.
//!
//! | module | endpoint it replaces |
//! | --- | --- |
//! | `credit` | `/calculate` on r-and-d-tax |
//! | `group` | `/consolidate` on group-tax |
//! | `vat` | `/prepare-return` on vat-reporting |
//! | `provision` | `/calculate` on tax-provision |
//! | `risk` | `/assess` on tax-risk |
//! | `grants` | `/recognize` on government-grants |
//! | `xbrl` | `/generate` on xbrl-reporting |

#![forbid(unsafe_code)]

pub mod credit;
pub mod grants;
pub mod group;
pub mod money;
pub mod provision;
pub mod risk;
pub mod vat;
pub mod xbrl;

pub use credit::{calculate as calculate_credit, ClaimRequest, ClaimResult, Expenditure};
pub use grants::{
    recognize as recognize_grant, GrantRecognition, GrantRequest, GrantResult, GrantType,
};
pub use group::{
    consolidate as consolidate_group_tax, GroupTaxRequest, GroupTaxResult, Subsidiary,
};
pub use money::{money, round_dp};
pub use provision::{
    calculate as calculate_provision, ProvisionRequest, ProvisionResult, TemporaryDifference,
};
pub use risk::{assess as assess_tax_risk, RiskLevel, TaxRiskAssessment, TaxRiskItem};
pub use vat::{
    prepare as prepare_vat_return, VatReturn, VatReturnRequest, VatTransaction, VAT_STANDARD_RATE,
};
pub use xbrl::{generate as generate_xbrl, FinancialConcept, XbrlRequest, XbrlResult};

#[cfg(test)]
mod tests;
