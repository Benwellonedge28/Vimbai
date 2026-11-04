#![forbid(unsafe_code)]
//! The same JSON door, taking and returning `serde_json::Value`.
//!
//! The assurance functions speak strings because PyO3 hands them a string. The HTTP
//! interface holds a parsed body, so these wrappers parse once and hand back a value.

use serde_json::Value;

fn through(f: fn(&str) -> Result<String, String>, payload: &Value) -> Result<Value, String> {
    let request = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    let answer = f(&request)?;
    serde_json::from_str(&answer).map_err(|e| e.to_string())
}

macro_rules! view {
    ($name:ident => $inner:ident) => {
        /// JSON value in, JSON value out.
        pub fn $name(payload: &Value) -> Result<Value, String> {
            through(super::assurance::$inner, payload)
        }
    };
}

view!(regulatory_ratio_from_json => regulatory_ratio_json);
view!(tax_risk_from_json => tax_risk_json);
view!(group_tax_from_json => group_tax_json);
view!(vat_return_from_json => vat_return_json);
view!(grant_from_json => grant_json);
view!(scorecard_from_json => scorecard_json);
view!(distress_from_json => distress_json);
view!(fraud_risk_from_json => fraud_risk_json);
view!(credit_risk_from_json => credit_risk_json);
view!(pension_from_json => pension_json);
view!(partnership_allocation_from_json => partnership_allocation_json);
view!(partnership_goodwill_from_json => partnership_goodwill_json);
view!(partner_buyout_from_json => partner_buyout_json);
