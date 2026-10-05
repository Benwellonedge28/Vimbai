#![forbid(unsafe_code)]
//! Capital budgeting & valuation math.
//!
//! Ported from net-present-value-service, internal-rate-return-service,
//! payback-period-service and discount-factor-service with exact formula
//! and iteration parity. The remaining capital/valuation services
//! (accounting-rate-return, cost-of-capital, business/merger-valuation,
//! EVA, CROIC, MVA, du-pont, asset-allocation, asset-turnover,
//! risk-return, sustainable-growth, initial-investment,
//! investment-appraisal, capital-budgeting, deal-structuring,
//! divestiture, post-merger, synergy-analysis) are queued to port here;
//! see `queued.rs` for their formula inventory.

pub mod dcf;
pub mod irr;
pub mod payback;
pub mod queued;

pub use dcf::{
    annuity_factor, discount_factor, discount_factor_percent, discount_factor_table,
    discount_factor_table_range, npv, npv_percent, npv_project_comparison, profitability_index,
    NpvDecision, NpvDetail, NpvOutput, ProjectNpv,
};
pub use irr::{irr, irr_bisection, npv_at_rate, IrrMethod, IrrOutput};
pub use payback::{payback_period, PaybackOutput};
