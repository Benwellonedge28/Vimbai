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
pub mod filings;
pub mod group;
pub mod rd_credit;
pub mod returns;
pub mod risk;
pub mod service;
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

/// `statutory-filing-service /assess`.
pub fn statutory_filing_assess_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    filings::assess(payload)
}

/// `tax-return-preparation-service /prepare`.
///
/// The service carried the fiscal year as text and added three fields the calculator does not
/// produce - the credits applied, a filing status and three standing recommendations. Those are
/// shaped here so the answer matches what clients already receive.
pub fn tax_return_preparation_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let differences = |key: &str| -> Vec<std::collections::BTreeMap<String, f64>> {
        payload
            .get(key)
            .and_then(|v| v.as_array())
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        row.as_object()
                            .map(|object| {
                                object
                                    .iter()
                                    .filter_map(|(k, v)| {
                                        v.as_f64().map(|number| (k.clone(), number))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let fiscal_year = match payload.get("fiscal_year") {
        Some(serde_json::Value::String(text)) => text.parse::<i64>().unwrap_or(0),
        Some(serde_json::Value::Number(number)) => number.as_i64().unwrap_or(0),
        _ => 0,
    };
    let credits: f64 = payload
        .get("tax_credits")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("credit_amount").and_then(|v| v.as_f64()))
                .sum()
        })
        .unwrap_or(0.0);

    let request = returns::TaxReturnRequest {
        company_id: payload
            .get("company_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        fiscal_year,
        accounting_profit: payload
            .get("accounting_profit")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        permanent_differences: differences("permanent_differences"),
        temporary_differences: differences("temporary_differences"),
        tax_credits: differences("tax_credits"),
    };
    let result = returns::prepare_tax_return(&request);
    let mut value = serde_json::to_value(&result).map_err(|err| err.to_string())?;
    let object = value.as_object_mut().ok_or("expected an object")?;
    object.insert(
        "fiscal_year".into(),
        payload
            .get("fiscal_year")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    );
    object.insert(
        "tax_credits_applied".into(),
        serde_json::json!(vimbai_common::numbers::round_dp(credits, 2)),
    );
    object.insert(
        "return_status".into(),
        serde_json::json!("ready_for_filing"),
    );
    object.insert(
        "recommendations".into(),
        serde_json::json!([
            "Review all tax positions",
            "Ensure documentation is complete",
            "File before deadline",
        ]),
    );
    Ok(value)
}

/// `tax-service /calculate`.
pub fn tax_calculate_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    service::calculate(payload)
}

/// `tax-service /provision`.
pub fn tax_provision_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    service::provision(payload)
}

/// `tax-service /transfer-pricing`.
pub fn tax_transfer_pricing_from_json(
    payload: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    service::transfer_pricing(payload)
}

/// `tax-service /compliance`.
pub fn tax_compliance_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    service::compliance(payload)
}
