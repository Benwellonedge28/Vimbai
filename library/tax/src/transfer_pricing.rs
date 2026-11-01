//! tax-service `/transfer-pricing`: comparable-uncontrolled-price test.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferPricingResult {
    pub company_id: String,
    pub arm_length_amount: f64,
    pub adjustment_required: f64,
    pub method_used: String,
    pub range_low: f64,
    pub range_high: f64,
    pub compliance_status: String,
}

/// `comparables` may be empty: the service divides by `max(len, 1)` and
/// falls back to the transaction amount when no "price" keys are present.
pub fn transfer_pricing(
    company_id: &str,
    transaction_amount: f64,
    comparables: &[BTreeMap<String, f64>],
) -> TransferPricingResult {
    let comparable_avg = if comparables.is_empty() {
        transaction_amount
    } else {
        comparables
            .iter()
            .map(|c| c.get("price").copied().unwrap_or(transaction_amount))
            .sum::<f64>()
            / comparables.len() as f64
    };

    let adjustment = transaction_amount - comparable_avg;
    let status = if adjustment.abs() < comparable_avg * 0.05 {
        "compliant"
    } else {
        "review_required"
    };

    TransferPricingResult {
        company_id: company_id.to_string(),
        arm_length_amount: py_round2(comparable_avg),
        adjustment_required: py_round2(adjustment),
        method_used: "Comparable Uncontrolled Price".to_string(),
        range_low: py_round2(comparable_avg * 0.9),
        range_high: py_round2(comparable_avg * 1.1),
        compliance_status: status.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp(price: f64) -> BTreeMap<String, f64> {
        let mut m = BTreeMap::new();
        m.insert("price".to_string(), price);
        m
    }

    #[test]
    fn within_five_percent_is_compliant() {
        let r = transfer_pricing("c1", 102_000.0, &[comp(100_000.0), comp(100_000.0)]);
        assert_eq!(r.arm_length_amount, 100_000.0);
        assert_eq!(r.adjustment_required, 2_000.0);
        assert_eq!(r.compliance_status, "compliant");
        assert_eq!(r.range_low, 90_000.0);
        assert_eq!(r.range_high, 110_000.0);
    }

    #[test]
    fn large_adjustment_requires_review() {
        let r = transfer_pricing("c1", 150_000.0, &[comp(100_000.0)]);
        assert_eq!(r.adjustment_required, 50_000.0);
        assert_eq!(r.compliance_status, "review_required");
    }

    #[test]
    fn missing_price_key_falls_back_to_transaction() {
        let r = transfer_pricing("c1", 120_000.0, &[BTreeMap::new()]);
        assert_eq!(r.arm_length_amount, 120_000.0);
        assert_eq!(r.adjustment_required, 0.0);
        assert_eq!(r.compliance_status, "compliant");
    }
}
