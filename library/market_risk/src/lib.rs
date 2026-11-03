//! Vimbai market and credit risk.
//!
//! Options pricing, VaR, credit rating with PD/LGD/EAD/ECL, fraud risk, distress
//! prediction and regulatory ratios. Stateless, no database, no clock.
//!
//! The normal distribution is implemented here rather than pulled from a
//! statistics crate, so the crate has no numeric dependency beyond serde.

#![forbid(unsafe_code)]

pub mod credit;
pub mod distress;
pub mod fraud;
pub mod money;
pub mod normal;
pub mod options;
pub mod regulatory;
pub mod var;

pub use credit::{
    analyze as analyze_credit_risk, financial_score, historical_pd, rate, Borrower, CreditRequest,
    CreditRiskResponse, Financials, HistoricalPd, Qualitative,
};
pub use distress::{predict as predict_distress, CompanyData, DistressPrediction};
pub use fraud::{assess as assess_fraud_risk, FraudAssessment, FraudAssessmentRequest};
pub use money::{money, round_dp};
pub use normal::{erf, norm_cdf, norm_pdf};
pub use options::{
    black_scholes_call, black_scholes_put, price as price_options, Option, PricingRequest,
    PricingResponse,
};
pub use regulatory::{
    calculate as calculate_regulatory, RegulatoryRatio, RegulatoryRequest, RegulatoryResponse,
};
pub use var::{calculate as calculate_var, Position, VarRequest, VarResult};

#[cfg(test)]
mod tests;
