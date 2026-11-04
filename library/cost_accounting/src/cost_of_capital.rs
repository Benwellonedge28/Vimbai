#![forbid(unsafe_code)]
//! cost-of-capital-service.
//!
//! The arithmetic already lives in `vimbai_capital_valuation::capital_returns`,
//! ported with the capital-returns family. This module is the service's entry
//! point to it rather than a second implementation, so the two cannot drift.
//! Only the rate table, which belongs to this service, is defined here.

pub use vimbai_capital_valuation::capital_returns::{
    compound_rate, cost_of_capital, cost_of_capital_percent, simple_rate, wacc, CompoundRateOutput,
    CostOfCapitalOutput, CostOfCapitalPercentOutput, RateOutput, WaccOutput,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StandardRate {
    pub name: String,
    pub description: String,
}

/// `/list` answers from a fixed table in the service.
pub fn standard_rates() -> Vec<StandardRate> {
    [
        ("Treasury Rate", "Risk-free rate"),
        ("Cost of Debt", "Interest rate on borrowed capital"),
        ("Cost of Equity", "Required return by shareholders"),
        ("WACC", "Weighted Average Cost of Capital"),
    ]
    .iter()
    .map(|(name, description)| StandardRate {
        name: name.to_string(),
        description: description.to_string(),
    })
    .collect()
}
