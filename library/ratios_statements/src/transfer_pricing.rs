//! transfer-pricing-service `/analyze`: OECD arm's-length testing of
//! intercompany transactions and the documentation checklist.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingMethod {
    #[default]
    ComparableUncontrolledPrice,
    ResalePrice,
    CostPlus,
    TransactionalNetMargin,
    ProfitSplit,
}

impl PricingMethod {
    pub fn value(&self) -> &'static str {
        match self {
            PricingMethod::ComparableUncontrolledPrice => "comparable_uncontrolled_price",
            PricingMethod::ResalePrice => "resale_price",
            PricingMethod::CostPlus => "cost_plus",
            PricingMethod::TransactionalNetMargin => "transactional_net_margin",
            PricingMethod::ProfitSplit => "profit_split",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransferTransaction {
    pub id: String,
    pub company_id: String,
    pub product_service: String,
    pub selling_entity: String,
    pub buying_entity: String,
    pub transaction_value: f64,
    pub cost_of_goods: f64,
    pub arm_length_range_min: f64,
    pub arm_length_range_max: f64,
    pub method: PricingMethod,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransferPricingInput {
    pub company_id: String,
    pub transactions: Vec<TransferTransaction>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransactionResult {
    pub id: String,
    pub product_service: String,
    pub selling_entity: String,
    pub buying_entity: String,
    pub transaction_value: f64,
    pub arm_length_range: (f64, f64),
    pub method: String,
    pub compliant: bool,
    pub adjustment_needed: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransferPricingAnalysis {
    pub company_id: String,
    pub compliant_transactions: usize,
    pub non_compliant_transactions: usize,
    pub total_adjustment_needed: f64,
    pub transactions: Vec<TransactionResult>,
    pub documentation_required: Vec<String>,
}

/// PYTHON QUIRK: the range test is inclusive at both ends, and because the
/// range defaults to 0..0 a zero-value transaction reads as compliant. The
/// per-transaction adjustment is signed (negative when the price is above
/// the range) even though the total sums absolute values, so a report can
/// show a negative adjustment against a positive total.
pub fn analyze_transfer_pricing(req: &TransferPricingInput) -> TransferPricingAnalysis {
    let mut compliant = 0usize;
    let mut non_compliant = 0usize;
    let mut total_adjustment = 0.0;
    let mut results = Vec::new();

    for tx in &req.transactions {
        let is_compliant = tx.arm_length_range_min <= tx.transaction_value
            && tx.transaction_value <= tx.arm_length_range_max;
        let adjustment = if is_compliant {
            compliant += 1;
            0.0
        } else {
            non_compliant += 1;
            if tx.transaction_value < tx.arm_length_range_min {
                tx.arm_length_range_min - tx.transaction_value
            } else {
                tx.arm_length_range_max - tx.transaction_value
            }
        };
        if !is_compliant {
            total_adjustment += adjustment.abs();
        }
        results.push(TransactionResult {
            id: tx.id.clone(),
            product_service: tx.product_service.clone(),
            selling_entity: tx.selling_entity.clone(),
            buying_entity: tx.buying_entity.clone(),
            transaction_value: tx.transaction_value,
            arm_length_range: (tx.arm_length_range_min, tx.arm_length_range_max),
            method: tx.method.value().to_string(),
            compliant: is_compliant,
            adjustment_needed: py_round2(adjustment),
        });
    }

    TransferPricingAnalysis {
        company_id: req.company_id.clone(),
        compliant_transactions: compliant,
        non_compliant_transactions: non_compliant,
        total_adjustment_needed: py_round2(total_adjustment),
        transactions: results,
        documentation_required: vec![
            "Master File (OECD)".to_string(),
            "Local File for each jurisdiction".to_string(),
            "Country-by-Country report (if revenue threshold met)".to_string(),
            "Benchmarking study supporting selected method".to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(id: &str, value: f64, lo: f64, hi: f64) -> TransferTransaction {
        TransferTransaction {
            id: id.into(),
            company_id: "c1".into(),
            product_service: "Widgets".into(),
            selling_entity: "ZW Co".into(),
            buying_entity: "KE Co".into(),
            transaction_value: value,
            cost_of_goods: 0.0,
            arm_length_range_min: lo,
            arm_length_range_max: hi,
            method: PricingMethod::CostPlus,
        }
    }

    fn input() -> TransferPricingInput {
        TransferPricingInput {
            company_id: "c1".into(),
            transactions: vec![
                tx("t1", 120.0, 100.0, 150.0),
                tx("t2", 80.0, 100.0, 150.0),
                tx("t3", 200.0, 100.0, 150.0),
            ],
        }
    }

    #[test]
    fn compliance_counts_and_adjustments() {
        let r = analyze_transfer_pricing(&input());
        assert_eq!(r.compliant_transactions, 1);
        assert_eq!(r.non_compliant_transactions, 2);
        // under-priced lifts by 20, over-priced reduces by 50
        assert_eq!(r.transactions[1].adjustment_needed, 20.0);
        assert_eq!(r.transactions[2].adjustment_needed, -50.0);
        // the total sums absolute values, so it is positive
        assert_eq!(r.total_adjustment_needed, 70.0);
    }

    #[test]
    fn range_bounds_are_inclusive() {
        let r = analyze_transfer_pricing(&TransferPricingInput {
            company_id: "c1".into(),
            transactions: vec![tx("lo", 100.0, 100.0, 150.0), tx("hi", 150.0, 100.0, 150.0)],
        });
        assert_eq!(r.compliant_transactions, 2);
        assert_eq!(r.total_adjustment_needed, 0.0);
    }

    #[test]
    fn default_zero_range_makes_zero_value_compliant() {
        let mut t = tx("t", 0.0, 0.0, 0.0);
        t.method = PricingMethod::default();
        let r = analyze_transfer_pricing(&TransferPricingInput {
            company_id: "c1".into(),
            transactions: vec![t],
        });
        assert_eq!(r.compliant_transactions, 1);
        assert_eq!(r.transactions[0].method, "comparable_uncontrolled_price");
    }

    #[test]
    fn methods_report_their_wire_values() {
        let r = analyze_transfer_pricing(&input());
        assert_eq!(r.transactions[0].method, "cost_plus");
        assert_eq!(PricingMethod::ProfitSplit.value(), "profit_split");
        assert_eq!(
            PricingMethod::TransactionalNetMargin.value(),
            "transactional_net_margin"
        );
        assert_eq!(PricingMethod::ResalePrice.value(), "resale_price");
    }

    #[test]
    fn documentation_checklist_is_fixed() {
        let r = analyze_transfer_pricing(&TransferPricingInput::default());
        assert_eq!(r.documentation_required.len(), 4);
        assert_eq!(r.documentation_required[0], "Master File (OECD)");
        assert_eq!(r.transactions.len(), 0);
    }
}
