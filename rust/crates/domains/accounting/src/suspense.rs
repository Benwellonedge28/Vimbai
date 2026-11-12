#![forbid(unsafe_code)]
//! Suspense and error, ported from suspense-error-service `/analyze`.
//!
//! Kept as shipped: what each error type does to the suspense account is a fixed
//! table, not a calculation. An omission corrects by its own amount, an
//! original-entry or complete-reversal error by twice it, and commission,
//! principle and compensating errors correct nothing at all - the last because
//! they are offsetting. Any unrecognised type corrects by its own amount.
//! The corrected balance is the suspense balance less those effects.
//!
//! Every amount is Money, so a suspense balance past double precision stays
//! exact; nothing here divides, so there is no ratio to keep.

use serde::{Deserialize, Serialize};
use vimbai_common::Money;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuspenseError {
    #[serde(default)]
    pub error_type: String,
    #[serde(default)]
    pub description: String,
    pub amount: Money,
    #[serde(default)]
    pub account_affected: String,
    #[serde(default)]
    pub correct_debit: Option<String>,
    #[serde(default)]
    pub correct_credit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuspenseRequest {
    pub company_id: String,
    #[serde(default)]
    pub period: String,
    pub suspense_balance: Money,
    #[serde(default)]
    pub errors: Vec<SuspenseError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Correction {
    pub error_type: String,
    pub description: String,
    pub amount: Money,
    pub effect_on_suspense: Money,
    pub account_affected: String,
    pub corrective_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SuspenseResult {
    pub company_id: String,
    pub period: String,
    pub original_suspense_balance: Money,
    pub total_errors_found: usize,
    pub corrected_suspense_balance: Money,
    pub corrections: Vec<Correction>,
}

/// What one error type does, and what the books keeper should do about it.
fn correction_for(error: &SuspenseError) -> (Money, String) {
    let amount = &error.amount;
    let twice = amount.times(&Money::parse("2").expect("two")).round(2);
    let nothing = Money::zero();
    match error.error_type.as_str() {
        "omission" => (
            amount.clone(),
            format!(
                "Record missing entry: Dr {}, Cr {}",
                error.correct_debit.as_deref().unwrap_or("N/A"),
                error.correct_credit.as_deref().unwrap_or("N/A")
            ),
        ),
        "commission" => (
            nothing,
            "Transfer from wrong to correct account".to_string(),
        ),
        "principle" => (
            nothing,
            format!(
                "Reclassify from {} to correct category",
                error.account_affected
            ),
        ),
        "original_entry" => (
            twice.clone(),
            format!(
                "Correct understated/overstated entry by {}",
                twice.to_fixed(2)
            ),
        ),
        "complete_reversal" => (
            twice,
            "Reverse and re-enter correctly (Dr/Cr swapped)".to_string(),
        ),
        "compensating" => (
            nothing,
            "Offsetting errors - no net suspense effect".to_string(),
        ),
        _ => (
            amount.clone(),
            format!("Correct entry for {}", error.description),
        ),
    }
}

/// `/analyze`
pub fn analyze_suspense(request: &SuspenseRequest) -> SuspenseResult {
    let mut corrections = Vec::with_capacity(request.errors.len());
    let mut total_corrections = Money::zero();

    for error in &request.errors {
        let (effect, action) = correction_for(error);
        total_corrections = total_corrections.plus(&effect);
        corrections.push(Correction {
            error_type: error.error_type.clone(),
            description: error.description.clone(),
            amount: error.amount.clone(),
            effect_on_suspense: effect,
            account_affected: error.account_affected.clone(),
            corrective_action: action,
        });
    }

    SuspenseResult {
        company_id: request.company_id.clone(),
        period: request.period.clone(),
        original_suspense_balance: request.suspense_balance.clone(),
        total_errors_found: request.errors.len(),
        corrected_suspense_balance: request.suspense_balance.minus(&total_corrections).round(2),
        corrections,
    }
}

/// `suspense-error-service /analyze`.
pub fn suspense_error_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: SuspenseRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(analyze_suspense(&request)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn fixed(amount: &Money) -> String {
        amount.to_fixed(2)
    }

    fn error(kind: &str, amount: &str, account: &str) -> SuspenseError {
        SuspenseError {
            error_type: kind.to_string(),
            description: format!("{kind} found"),
            amount: m(amount),
            account_affected: account.to_string(),
            correct_debit: Some("Purchases".to_string()),
            correct_credit: Some("Suspense".to_string()),
        }
    }

    fn request() -> SuspenseRequest {
        SuspenseRequest {
            company_id: "c1".to_string(),
            period: "2026-11".to_string(),
            suspense_balance: m("1000"),
            errors: vec![
                error("omission", "150", "Sales"),
                error("commission", "200", "Rent"),
                error("original_entry", "50", "Wages"),
            ],
        }
    }

    #[test]
    fn each_error_type_moves_the_balance_by_its_fixed_table() {
        let out = analyze_suspense(&request());
        assert_eq!(out.total_errors_found, 3);
        assert_eq!(fixed(&out.corrections[0].effect_on_suspense), "150.00");
        assert_eq!(fixed(&out.corrections[1].effect_on_suspense), "0.00");
        assert_eq!(fixed(&out.corrections[2].effect_on_suspense), "100.00");
        // 1000 - (150 + 0 + 100)
        assert_eq!(fixed(&out.corrected_suspense_balance), "750.00");
        assert_eq!(fixed(&out.original_suspense_balance), "1000.00");
    }

    #[test]
    fn the_actions_name_the_accounts_as_the_service_did() {
        let out = analyze_suspense(&request());
        assert_eq!(
            out.corrections[0].corrective_action,
            "Record missing entry: Dr Purchases, Cr Suspense"
        );
        assert_eq!(
            out.corrections[1].corrective_action,
            "Transfer from wrong to correct account"
        );
        assert_eq!(
            out.corrections[2].corrective_action,
            "Correct understated/overstated entry by 100.00"
        );
    }

    #[test]
    fn compensating_principle_and_reversal_keep_their_shape() {
        let out = analyze_suspense(&SuspenseRequest {
            company_id: "c1".to_string(),
            period: "2026-11".to_string(),
            suspense_balance: m("500"),
            errors: vec![
                error("principle", "80", "Motor"),
                error("compensating", "90", "Sundry"),
                error("complete_reversal", "25", "Bank"),
                error("something-new", "40", "Other"),
            ],
        });
        assert_eq!(fixed(&out.corrections[0].effect_on_suspense), "0.00");
        assert_eq!(
            out.corrections[0].corrective_action,
            "Reclassify from Motor to correct category"
        );
        assert_eq!(
            out.corrections[1].corrective_action,
            "Offsetting errors - no net suspense effect"
        );
        assert_eq!(fixed(&out.corrections[2].effect_on_suspense), "50.00");
        assert_eq!(
            out.corrections[2].corrective_action,
            "Reverse and re-enter correctly (Dr/Cr swapped)"
        );
        // an unknown type corrects by its own amount
        assert_eq!(fixed(&out.corrections[3].effect_on_suspense), "40.00");
        assert_eq!(
            out.corrections[3].corrective_action,
            "Correct entry for something-new found"
        );
        assert_eq!(fixed(&out.corrected_suspense_balance), "410.00");
    }

    #[test]
    fn a_missing_account_falls_back_to_not_available() {
        let mut without = error("omission", "10", "Sales");
        without.correct_debit = None;
        without.correct_credit = None;
        let out = analyze_suspense(&SuspenseRequest {
            company_id: "c1".to_string(),
            period: String::new(),
            suspense_balance: m("10"),
            errors: vec![without],
        });
        assert_eq!(
            out.corrections[0].corrective_action,
            "Record missing entry: Dr N/A, Cr N/A"
        );
    }

    #[test]
    fn a_balance_past_double_precision_stays_exact() {
        let out = analyze_suspense(&SuspenseRequest {
            company_id: "c1".to_string(),
            period: "2026-11".to_string(),
            suspense_balance: m("123456789012345678901.55"),
            errors: vec![error("original_entry", "0.55", "Wages")],
        });
        // twice 0.55 comes off the balance, digit for digit
        assert_eq!(fixed(&out.corrections[0].effect_on_suspense), "1.10");
        assert_eq!(
            fixed(&out.corrected_suspense_balance),
            "123456789012345678900.45"
        );
    }

    #[test]
    fn the_door_reads_the_published_shape() {
        let payload = serde_json::json!({
            "company_id": "c1", "period": "2026-11", "suspense_balance": "1000",
            "errors": [
                {"error_type": "omission", "description": "d", "amount": "150",
                 "account_affected": "Sales", "correct_debit": "Purchases",
                 "correct_credit": "Suspense"},
                {"error_type": "commission", "description": "d", "amount": "200",
                 "account_affected": "Rent"}
            ]
        });
        let out = suspense_error_from_json(&payload).expect("door");
        assert_eq!(out["corrected_suspense_balance"], "850.00");
        assert_eq!(out["total_errors_found"], 2);

        let bad = suspense_error_from_json(&serde_json::json!({
            "company_id": "c1", "suspense_balance": "lots"
        }))
        .expect_err("refused");
        assert!(bad.contains("invalid request"), "{bad}");
    }
}
