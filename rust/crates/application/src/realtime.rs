#![forbid(unsafe_code)]
//! realtime-calculation-engine: four figures, each answered by the calculator that already owns it.
//!
//! This module is a door, not a calculator. Every figure here exists elsewhere in the workspace, and the point of this
//! door is that it calls the one that owns it rather than working anything out again:
//!
//! * [`npv`] is the discounting engine the four capital-budgeting services share, reached with the engine's own habit of
//!   stating a rate as a percent;
//! * [`irr`] is the one rate solver, with this engine's iteration budget and tolerance, and it therefore carries the
//!   verification that stops a rate being published before it is checked against the net it produces;
//! * [`depreciation_schedule`] is the assets crate's three-method schedule, beside the per-method arithmetic that crate already
//!   held;
//! * [`amortize`] is the loan paydown the stored loans walk, in this engine's own arrival shape.
//!
//! Nothing here stores a record and nothing here reads one, so there is no Book scope to check. A figure is a figure.

use serde_json::Value;

use vimbai_store::StoreError;

/// `POST /npv`: what a series is worth today, once the investment is paid for, at a rate stated as a percent.
pub fn npv(payload: &Value) -> Result<Value, StoreError> {
    vimbai_service_finance::discounting::quick_npv(payload)
}

/// `POST /irr`: the rate at which the series is worth exactly what it cost.
pub fn irr(payload: &Value) -> Result<Value, StoreError> {
    vimbai_capital_valuation::irr::quick_irr_from_json(payload).map_err(StoreError::Invalid)
}

/// `POST /depreciation`: a year-by-year schedule by straight line, declining balance or sum of years.
pub fn depreciation_schedule(payload: &Value) -> Result<Value, StoreError> {
    vimbai_assets::depreciation::quick_depreciation(payload)
}

/// `POST /amortize`: a loan paid down month by month.
pub fn amortize(payload: &Value) -> Result<Value, StoreError> {
    vimbai_service_treasury::debt_management::quick_amortisation(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_four_doors_answer_with_the_figures_the_calculators_own() {
        assert_eq!(
            npv(&json!({"initial_investment": 1000, "cash_flows": [1100], "discount_rate": 10}))
                .expect("a net")["npv"],
            json!(0.0)
        );
        assert_eq!(
            irr(&json!({"initial_investment": 1000, "cash_flows": [1100]})).expect("a rate")["irr"],
            json!(10.0)
        );
        assert_eq!(
            depreciation_schedule(&json!({"cost": 900, "salvage_value": 0, "useful_life": 3, "method": "straight_line"}))
                .expect("a schedule")["schedule"][0]["depreciation"],
            json!(300.0)
        );
        assert_eq!(
            amortize(&json!({"principal": 1200, "annual_rate": 0, "years": 1}))
                .expect("a schedule")["monthly_payment"],
            json!(100.0)
        );

        // the doors refuse the same things the calculators do, rather than answering a guess
        assert!(irr(&json!({"initial_investment": 1000, "cash_flows": []})).is_err());
        assert!(amortize(&json!({"principal": 0, "annual_rate": 5, "years": 1})).is_err());
    }
}
