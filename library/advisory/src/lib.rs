#![forbid(unsafe_code)]
//! Advisory endpoints: the analysis services that keep no record of their own.
//!
//! Every function here replaces a container that did exactly one thing — take
//! the caller's numbers, work out a report, return it. Bodies are JSON values
//! because the originals answered with loose `Dict[str, Any]` shapes.

pub mod audit;
pub mod contracts;
pub mod portfolios;
pub mod studies;
pub mod treasury;

use serde_json::Value;
use vimbai_common::py_round2;

pub fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

pub fn rows<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn r2(value: f64) -> f64 {
    py_round2(value)
}

pub fn div(num: f64, den: f64) -> f64 {
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

/// Python's `f"{value:.2f}"`, used inside the sweep description.
pub fn py_float2(value: f64) -> String {
    format!("{value:.2}")
}

/// The services stamped a uuid on their answers; the caller supplies its own.
pub fn with_id(mut answer: Value, id: &str) -> Value {
    if let Value::Object(ref mut map) = answer {
        map.insert("id".to_string(), Value::String(id.to_string()));
    }
    answer
}

pub use audit::{analyze_analytical_procedures, analyze_transfer_pricing};
pub use contracts::analyze_construction_contracts;
pub use portfolios::{
    analyze_fund_management, analyze_interest_rate_risk, analyze_investment_portfolio,
};
pub use studies::{
    analyze_events_after_reporting, analyze_household, analyze_profitability_index,
    analyze_related_parties, analyze_share_options,
};
pub use treasury::{analyze_cash_pooling, analyze_debt_covenants, analyze_director_emoluments};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Python and Rust agree on arithmetic, not on float widening.
    fn normalise(value: &Value) -> Value {
        match value {
            Value::Number(n) => json!(n.as_f64().unwrap_or(0.0)),
            Value::Array(items) => Value::Array(items.iter().map(normalise).collect()),
            Value::Object(map) => {
                Value::Object(map.iter().map(|(k, v)| (k.clone(), normalise(v))).collect())
            }
            other => other.clone(),
        }
    }

    fn assert_parity(actual: Value, expected: &str) {
        let expected: Value = serde_json::from_str(expected).expect("expected json");
        assert_eq!(
            serde_json::to_string(&normalise(&actual)).unwrap(),
            serde_json::to_string(&normalise(&expected)).unwrap()
        );
    }

    #[test]
    fn cash_pooling_matches_the_service() {
        let request = json!({
            "company_id": "C1", "pooling_type": "zero_balance", "target_account": "T1",
            "threshold_amount": 100,
            "accounts": [
                {"account_id": "A1", "account_name": "Main", "balance": 500, "currency": "USD",
                 "bank": "B1", "is_pooled": true},
                {"account_id": "A2", "account_name": "Ops", "balance": 150, "currency": "USD",
                 "bank": "B1", "is_pooled": true},
                {"account_id": "A3", "account_name": "Payroll", "balance": 40, "currency": "USD",
                 "bank": "B2", "is_pooled": false},
                {"account_id": "A4", "account_name": "Tax", "balance": -20, "currency": "USD",
                 "bank": "B2", "is_pooled": false}
            ]
        });
        let expected = r#"{
            "company_id": "C1",
            "pool_summary": {"total_accounts": 4, "pooled_accounts": 2, "non_pooled_accounts": 2,
                             "total_pooled_cash": 650.0, "total_non_pooled_cash": 20.0,
                             "total_cash": 670.0, "pooling_efficiency": 97.01},
            "account_analysis": [
                {"account_id": "A1", "account_name": "Main", "balance": 500.0, "currency": "USD",
                 "bank": "B1", "is_pooled": true, "surplus_deficit": 400.0},
                {"account_id": "A2", "account_name": "Ops", "balance": 150.0, "currency": "USD",
                 "bank": "B1", "is_pooled": true, "surplus_deficit": 50.0},
                {"account_id": "A3", "account_name": "Payroll", "balance": 40.0, "currency": "USD",
                 "bank": "B2", "is_pooled": false, "surplus_deficit": -60.0},
                {"account_id": "A4", "account_name": "Tax", "balance": -20.0, "currency": "USD",
                 "bank": "B2", "is_pooled": false, "surplus_deficit": -120.0}
            ],
            "optimization_opportunities": [],
            "recommendations": []
        }"#;
        assert_parity(analyze_cash_pooling(&request), expected);
    }

    #[test]
    fn cash_pooling_sweeps_the_excess_and_warns_on_thin_pooling() {
        let request = json!({
            "company_id": "C1", "threshold_amount": 50,
            "accounts": [
                {"account_id": "A1", "account_name": "Main", "balance": 100, "currency": "USD",
                 "bank": "B1", "is_pooled": true},
                {"account_id": "A2", "account_name": "Ops", "balance": 60, "currency": "USD",
                 "bank": "B2", "is_pooled": false},
                {"account_id": "A3", "account_name": "Tax", "balance": 40, "currency": "USD",
                 "bank": "B2", "is_pooled": false},
                {"account_id": "A4", "account_name": "VAT", "balance": 20, "currency": "USD",
                 "bank": "B3", "is_pooled": false},
                {"account_id": "A5", "account_name": "Levy", "balance": 10, "currency": "USD",
                 "bank": "B3", "is_pooled": false}
            ]
        });
        let out = analyze_cash_pooling(&request);
        assert_eq!(out["pool_summary"]["pooling_efficiency"], json!(43.48));
        assert_eq!(
            out["optimization_opportunities"][0]["type"],
            json!("sweep_excess")
        );
        assert_eq!(out["optimization_opportunities"][0]["amount"], json!(80.0));
        assert_eq!(
            out["optimization_opportunities"][0]["description"],
            json!("Sweep 80.00 to central pool")
        );
        // four non-pooled accounts: one past the three-account comfort zone
        assert_eq!(
            out["optimization_opportunities"][1]["accounts_to_close"],
            json!(1)
        );
        assert_eq!(
            out["recommendations"],
            json!(["Improve pooling efficiency by including more accounts"])
        );
    }

    fn covenant_request() -> Value {
        json!({"company_id": "C1", "total_debt": 600, "ebitda": 200, "interest_expense": 50,
               "current_assets": 400, "current_liabilities": 300, "tangible_net_worth": 350,
               "total_assets": 1200, "equity": 300, "net_income": 60, "revenue": 1000})
    }

    #[test]
    fn debt_covenants_match_the_service() {
        let out = analyze_debt_covenants(&covenant_request(), "2026-11-03T09:00:00");
        assert_eq!(out["overall_compliance"], json!(true));
        assert_eq!(out["covenant_count"], json!(5));
        assert_eq!(out["compliant_count"], json!(5));
        assert_eq!(out["breached_count"], json!(0));
        // the exactly-met D/E covenant leaves zero headroom, so it warns
        assert_eq!(out["warning_count"], json!(1));
        assert_eq!(out["breach_risk"], json!("LOW"));
        assert_eq!(
            out["covenants"][0]["metrics"][0]["actual_value"],
            json!(2.0)
        );
        assert_eq!(out["covenants"][0]["metrics"][0]["compliant"], json!(true));
        assert_eq!(
            out["covenants"][2]["metrics"][0]["headroom_percentage"],
            json!(60.0)
        );
        assert_eq!(
            out["covenants"][3]["metrics"][0]["actual_value"],
            json!(1.3333)
        );
        assert_eq!(out["recommendations"], json!([]));
    }

    #[test]
    fn covenant_breach_is_high_risk() {
        let request = json!({"company_id": "C1", "total_debt": 900, "ebitda": 150,
                             "interest_expense": 60, "current_assets": 200, "current_liabilities": 300,
                             "tangible_net_worth": 100, "total_assets": 1200, "equity": 300,
                             "net_income": 10, "revenue": 900});
        let out = analyze_debt_covenants(&request, "2026-11-03T09:00:00");
        assert_eq!(out["overall_compliance"], json!(false));
        // D/E 3.0, debt/EBITDA 6.0, current ratio 0.67 and TNW 0.083 all fail;
        // interest coverage exactly meets its 2.5 test.
        assert_eq!(out["breached_count"], json!(4));
        assert_eq!(out["compliant_count"], json!(1));
        assert_eq!(out["breach_risk"], json!("HIGH"));
        assert_eq!(
            out["recommendations"],
            json!([
                "URGENT: Covenant breach detected - engage with lenders immediately",
                "Approaching leverage limit - consider debt reduction strategies",
                "Interest coverage declining - monitor closely and consider refinancing"
            ])
        );
    }

    #[test]
    fn director_emoluments_match_the_service() {
        let request = json!({
            "company_id": "C1", "period": "FY2026", "highest_paid_director": "D1",
            "employee_count": 100, "total_staff_costs": 2000000,
            "directors": [
                {"id": "D1", "name": "Sam", "role": "CEO", "base_salary": 120000, "bonuses": 40000,
                 "share_options_value": 20000, "pension_contributions": 12000, "benefits": 8000},
                {"id": "D2", "name": "Ada", "role": "CFO", "base_salary": 90000, "bonuses": 10000,
                 "share_options_value": 0, "pension_contributions": 9000, "benefits": 5000}
            ]
        });
        let expected = r#"{
            "company_id": "C1",
            "period": "FY2026",
            "director_emoluments": [
                {"director_id": "D1", "director_name": "Sam", "role": "CEO", "base_salary": 120000.0,
                 "bonuses": 40000.0, "share_options_value": 20000.0,
                 "pension_contributions": 12000.0, "benefits": 8000.0, "total_emoluments": 200000.0},
                {"director_id": "D2", "director_name": "Ada", "role": "CFO", "base_salary": 90000.0,
                 "bonuses": 10000.0, "share_options_value": 0.0,
                 "pension_contributions": 9000.0, "benefits": 5000.0, "total_emoluments": 114000.0}
            ],
            "total_emoluments": 314000.0,
            "highest_paid": 200000.0,
            "average_emoluments": 157000.0,
            "emoluments_to_staff_costs": 0.157,
            "ceo_pay_ratio": 1.27,
            "disclosure_compliant": true,
            "recommendations": [
                "Ensure all director emoluments are fully disclosed",
                "Include equity-settled share-based payments at fair value",
                "Disclose performance criteria for bonus payments",
                "Consider CEO pay ratio disclosure per regulatory requirements"
            ]
        }"#;
        assert_parity(analyze_director_emoluments(&request), expected);
    }

    #[test]
    fn construction_contracts_match_the_service() {
        let request = json!({
            "company_id": "C1", "period": "2026-10", "accounting_method": "percentage_of_completion",
            "contracts": [
                {"id": "K1", "contract_value": 1000, "costs_incurred": 300,
                 "estimated_total_costs": 750, "completed": false},
                {"id": "K2", "contract_value": 500, "costs_incurred": 600,
                 "estimated_total_costs": 650, "completed": true},
                {"id": "K3", "contract_value": 200, "costs_incurred": 50}
            ]
        });
        let expected = r#"{
            "company_id": "C1",
            "period": "2026-10",
            "contract_analysis": [
                {"contract_id": "K1", "contract_value": 1000.0, "costs_incurred": 300.0,
                 "estimated_total_costs": 750.0, "progress_percentage": 40.0,
                 "revenue_to_date": 400.0, "recognized_profit": 100.0,
                 "expected_profit": 250.0, "provision_for_losses": 0.0},
                {"contract_id": "K2", "contract_value": 500.0, "costs_incurred": 600.0,
                 "estimated_total_costs": 650.0, "progress_percentage": 92.31,
                 "revenue_to_date": 461.54, "recognized_profit": 0.0,
                 "expected_profit": -150.0, "provision_for_losses": 150.0},
                {"contract_id": "K3", "contract_value": 200.0, "costs_incurred": 50.0,
                 "estimated_total_costs": 50.0, "progress_percentage": 100.0,
                 "revenue_to_date": 200.0, "recognized_profit": 150.0,
                 "expected_profit": 150.0, "provision_for_losses": 0.0}
            ],
            "total_contract_value": 1700.0,
            "total_revenue_recognized": 1061.54,
            "total_provision_for_losses": 150.0,
            "completed_contracts": 1,
            "ongoing_contracts": 2,
            "recommendations": [
                "Review cost estimates for contracts with low progress",
                "Recognize losses immediately when identified",
                "Ensure percentage of completion calculations are accurate"
            ]
        }"#;
        assert_parity(analyze_construction_contracts(&request), expected);
    }
}
