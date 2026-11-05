//! Net realisable value, scrap value and straight-line residual value.
//!
//! Every figure here is a subtraction of amounts: what a thing will fetch, less
//! what it costs to finish it and sell it, or proceeds less the cost of disposal.
//! Those are decimals so that a valuation does not inherit a double's drift, and
//! so that a write-down to NRV is the exact shortfall of cost below it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use vimbai_common::numbers::python_float;
use vimbai_common::Money;

/// A number from the service payload, as the decimal the library works in.
fn decimal(value: f64) -> Money {
    Money::parse(&value.to_string()).unwrap_or_else(|_| Money::zero())
}

/// A typed float from a JSON payload.
fn number(payload: &Value, key: &str) -> Money {
    match payload.get(key) {
        Some(Value::Number(n)) => decimal(n.as_f64().unwrap_or(0.0)),
        Some(Value::String(text)) => Money::parse(text).unwrap_or_else(|_| Money::zero()),
        _ => Money::zero(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NrvResult {
    pub selling_price: f64,
    pub completion_cost: f64,
    pub selling_cost: f64,
    pub net_realizable_value: f64,
    pub formula: String,
}

/// NRV = selling price less completion and selling costs.
///
/// The formula string is built from the typed inputs rather than the decimal
/// difference, because it only has to read like the numbers the caller sent.
pub fn net_realizable_value(
    selling_price: Money,
    completion_cost: Money,
    selling_cost: Money,
) -> NrvResult {
    let nrv = selling_price.minus(&completion_cost).minus(&selling_cost);
    let nrv_float = nrv.to_f64_lossy();
    NrvResult {
        selling_price: selling_price.to_f64_lossy(),
        completion_cost: completion_cost.to_f64_lossy(),
        selling_cost: selling_cost.to_f64_lossy(),
        net_realizable_value: nrv_float,
        formula: format!(
            "{} - {} - {} = {}",
            python_float(selling_price.to_f64_lossy()),
            python_float(completion_cost.to_f64_lossy()),
            python_float(selling_cost.to_f64_lossy()),
            python_float(nrv_float)
        ),
    }
}

/// `/calculate-nrv`, accepting the two capitalised spellings the service used.
pub fn net_realizable_value_from_json(payload: &Value) -> Result<Value, String> {
    let pick = |lower: &str, upper: &str| {
        if payload.get(lower).is_some() {
            number(payload, lower)
        } else {
            number(payload, upper)
        }
    };
    let result = net_realizable_value(
        pick("selling_price", "selling_Price"),
        pick("completion_cost", "Completion_Cost"),
        number(payload, "selling_cost"),
    );
    serde_json::to_value(result).map_err(|e| e.to_string())
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
    original_cost: Money,
    accumulated_depreciation: Money,
    disposal_cost: Money,
    proceeds_from_sale: Money,
) -> ScrapValueResult {
    let net_book_value = original_cost.minus(&accumulated_depreciation);
    let scrap = if proceeds_from_sale > Money::zero() {
        proceeds_from_sale.minus(&disposal_cost)
    } else {
        Money::zero()
    };
    ScrapValueResult {
        original_cost: original_cost.round(2).to_f64_lossy(),
        accumulated_depreciation: accumulated_depreciation.round(2).to_f64_lossy(),
        net_book_value: net_book_value.round(2).to_f64_lossy(),
        disposal_cost: disposal_cost.round(2).to_f64_lossy(),
        proceeds_from_sale: proceeds_from_sale.round(2).to_f64_lossy(),
        scrap_value: scrap.round(2).to_f64_lossy(),
        gain_loss_on_disposal: scrap.minus(&net_book_value).round(2).to_f64_lossy(),
    }
}

/// `/scrap-value`.
pub fn scrap_value_from_json(payload: &Value) -> Result<Value, String> {
    let result = scrap_value(
        number(payload, "original_cost"),
        number(payload, "accumulated_depreciation"),
        number(payload, "disposal_cost"),
        number(payload, "proceeds_from_sale"),
    );
    serde_json::to_value(result).map_err(|e| e.to_string())
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
    original_cost: Money,
    depreciation_rate: f64,
    useful_life_years: i32,
) -> ResidualValueResult {
    let rate = decimal(depreciation_rate);
    let total_depreciation = original_cost
        .times(&rate)
        .times(&Money::from_i64(useful_life_years as i64));
    let residual = original_cost.minus(&total_depreciation);
    ResidualValueResult {
        original_cost: original_cost.round(2).to_f64_lossy(),
        depreciation_rate,
        useful_life_years,
        total_depreciation: total_depreciation.round(2).to_f64_lossy(),
        residual_value: if residual > Money::zero() {
            residual.round(2).to_f64_lossy()
        } else {
            Money::zero().round(2).to_f64_lossy()
        },
    }
}

/// `/residual-value`.
pub fn residual_value_from_json(payload: &Value) -> Result<Value, String> {
    let result = residual_value(
        number(payload, "original_cost"),
        payload
            .get("depreciation_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        payload
            .get("useful_life_years")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32,
    );
    serde_json::to_value(result).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InventoryNrvResult {
    pub estimated_selling_price: f64,
    pub estimated_completion_cost: f64,
    pub estimated_selling_expenses: f64,
    pub net_realizable_value: f64,
    pub accounting_treatment: String,
}

/// `/NRV-inventory`: the NRV that inventory is written down to.
pub fn inventory_nrv(
    estimated_selling_price: Money,
    estimated_completion_cost: Money,
    estimated_selling_expenses: Money,
) -> InventoryNrvResult {
    let nrv = estimated_selling_price
        .minus(&estimated_completion_cost)
        .minus(&estimated_selling_expenses);
    InventoryNrvResult {
        estimated_selling_price: estimated_selling_price.round(2).to_f64_lossy(),
        estimated_completion_cost: estimated_completion_cost.round(2).to_f64_lossy(),
        estimated_selling_expenses: estimated_selling_expenses.round(2).to_f64_lossy(),
        net_realizable_value: nrv.round(2).to_f64_lossy(),
        accounting_treatment: "Write down inventory to NRV if NRV < Cost".to_string(),
    }
}

/// `/NRV-inventory`.
pub fn inventory_nrv_from_json(payload: &Value) -> Result<Value, String> {
    let result = inventory_nrv(
        number(payload, "estimated_selling_price"),
        number(payload, "estimated_completion_cost"),
        number(payload, "estimated_selling_expenses"),
    );
    serde_json::to_value(result).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompareCostNrvResult {
    pub cost: f64,
    pub selling_price: f64,
    pub completion_cost: f64,
    pub selling_cost: f64,
    pub net_realizable_value: f64,
    pub lower_of_cost_or_nrv: f64,
    pub write_down_required: bool,
    pub write_down_amount: f64,
}

/// `/compare-cost-nrv`: the lower of cost and NRV, with the write-down to take.
///
/// Preserved quirk: a zero or negative NRV leaves the cost in place rather than
/// valuing the stock at nothing, so the endpoint reports cost as the carrying
/// amount while still flagging a write-down of the full cost.
pub fn compare_cost_nrv(
    cost: Money,
    selling_price: Money,
    completion_cost: Money,
    selling_cost: Money,
) -> CompareCostNrvResult {
    let nrv = selling_price.minus(&completion_cost).minus(&selling_cost);
    let lower = if nrv > Money::zero() {
        if nrv < cost {
            nrv.clone()
        } else {
            cost.clone()
        }
    } else {
        cost.clone()
    };
    let write_down = cost.minus(&nrv);
    CompareCostNrvResult {
        cost: cost.round(2).to_f64_lossy(),
        selling_price: selling_price.round(2).to_f64_lossy(),
        completion_cost: completion_cost.round(2).to_f64_lossy(),
        selling_cost: selling_cost.round(2).to_f64_lossy(),
        net_realizable_value: nrv.round(2).to_f64_lossy(),
        lower_of_cost_or_nrv: lower.round(2).to_f64_lossy(),
        write_down_required: cost > nrv,
        write_down_amount: if write_down > Money::zero() {
            write_down.round(2).to_f64_lossy()
        } else {
            Money::zero().round(2).to_f64_lossy()
        },
    }
}

/// `/compare-cost-nrv`.
pub fn compare_cost_nrv_from_json(payload: &Value) -> Result<Value, String> {
    let result = compare_cost_nrv(
        number(payload, "cost"),
        number(payload, "selling_price"),
        number(payload, "completion_cost"),
        number(payload, "selling_cost"),
    );
    serde_json::to_value(result).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nrv_formula() {
        let r = net_realizable_value(
            Money::from_i64(100),
            Money::from_i64(15),
            Money::from_i64(5),
        );
        assert_eq!(r.net_realizable_value, 80.0);
        assert_eq!(r.formula, "100.0 - 15.0 - 5.0 = 80.0");
    }

    #[test]
    fn nrv_subtracts_exactly() {
        let r = net_realizable_value(
            Money::parse("100.10").unwrap(),
            Money::parse("15.05").unwrap(),
            Money::parse("5.02").unwrap(),
        );
        assert_eq!(r.net_realizable_value, 80.03);
    }

    #[test]
    fn scrap_zero_without_proceeds() {
        let r = scrap_value(
            Money::from_i64(10000),
            Money::from_i64(4000),
            Money::from_i64(500),
            Money::zero(),
        );
        assert_eq!(r.net_book_value, 6000.0);
        assert_eq!(r.scrap_value, 0.0);
        assert_eq!(r.gain_loss_on_disposal, -6000.0);
    }

    #[test]
    fn residual_floored_at_zero() {
        let r = residual_value(Money::from_i64(10000), 0.3, 5);
        assert_eq!(r.total_depreciation, 15000.0);
        assert_eq!(r.residual_value, 0.0);
    }

    #[test]
    fn cost_nrv_write_down() {
        let r = compare_cost_nrv(
            Money::from_i64(100),
            Money::from_i64(100),
            Money::from_i64(10),
            Money::from_i64(5),
        );
        assert_eq!(r.net_realizable_value, 85.0);
        assert_eq!(r.lower_of_cost_or_nrv, 85.0);
        assert!(r.write_down_required);
        assert_eq!(r.write_down_amount, 15.0);
        assert!(
            !compare_cost_nrv(
                Money::from_i64(100),
                Money::from_i64(100),
                Money::from_i64(0),
                Money::from_i64(0)
            )
            .write_down_required
        );
    }
}
