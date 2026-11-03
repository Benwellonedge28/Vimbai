//! Net realisable value, scrap value and straight-line residual value.

use serde::{Deserialize, Serialize};

use crate::money::{money, python_float};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NrvResult {
    pub selling_price: f64,
    pub completion_cost: f64,
    pub selling_cost: f64,
    pub net_realizable_value: f64,
    pub formula: String,
}

/// NRV = selling price less completion and selling costs.
pub fn net_realizable_value(
    selling_price: f64,
    completion_cost: f64,
    selling_cost: f64,
) -> NrvResult {
    let nrv = selling_price - completion_cost - selling_cost;
    NrvResult {
        selling_price,
        completion_cost,
        selling_cost,
        net_realizable_value: nrv,
        formula: format!(
            "{} - {} - {} = {}",
            python_float(selling_price),
            python_float(completion_cost),
            python_float(selling_cost),
            python_float(nrv)
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrapValueResult {
    pub original_cost: f64,
    pub accumulated_depreciation: f64,
    pub net_book_value: f64,
    pub disposal_cost: f64,
    pub proceeds_from_sale: f64,
    pub scrap_value: f64,
    pub gain_loss_on_disposal: f64,
}

/// Scrap value.
///
/// Preserved quirk: without proceeds there is no scrap value at all, so an asset
/// scrapped at a loss against a disposal cost reports zero rather than the cost.
pub fn scrap_value(
    original_cost: f64,
    accumulated_depreciation: f64,
    disposal_cost: f64,
    proceeds_from_sale: f64,
) -> ScrapValueResult {
    let net_book_value = original_cost - accumulated_depreciation;
    let scrap = if proceeds_from_sale > 0.0 {
        proceeds_from_sale - disposal_cost
    } else {
        0.0
    };
    ScrapValueResult {
        original_cost: money(original_cost),
        accumulated_depreciation: money(accumulated_depreciation),
        net_book_value: money(net_book_value),
        disposal_cost: money(disposal_cost),
        proceeds_from_sale: money(proceeds_from_sale),
        scrap_value: money(scrap),
        gain_loss_on_disposal: money(scrap - net_book_value),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResidualValueResult {
    pub original_cost: f64,
    pub depreciation_rate: f64,
    pub useful_life_years: i32,
    pub total_depreciation: f64,
    pub residual_value: f64,
}

/// Straight-line residual value, floored at zero: a rate and life that would
/// depreciate past the cost report nothing rather than a negative asset.
pub fn residual_value(
    original_cost: f64,
    depreciation_rate: f64,
    useful_life_years: i32,
) -> ResidualValueResult {
    let total_depreciation = original_cost * depreciation_rate * useful_life_years as f64;
    ResidualValueResult {
        original_cost: money(original_cost),
        depreciation_rate,
        useful_life_years,
        total_depreciation: money(total_depreciation),
        residual_value: money((original_cost - total_depreciation).max(0.0)),
    }
}
