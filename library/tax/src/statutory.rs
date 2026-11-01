//! corporate-tax-service: statutory corporate tax on accounting profit.
//!
//! Distinct from `corporate.rs`, which carries tax-calculation-service's
//! progressive bracket engine. This one applies a flat statutory rate to an
//! adjusted profit, nets off credits and spreads the liability over four
//! quarterly installments.
//!
//! Two behaviours are carried over from the Python service rather than
//! corrected, because the statements filed against them expect these numbers:
//! `effective_rate` divides the *net* liability by accounting profit (so it
//! falls when credits are applied), and a zero accounting profit reports a
//! zero effective rate instead of dividing by zero.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxAdjustment {
    /// The service calls this `type`; `kind` stays as the Rust-side name.
    #[serde(rename = "type", alias = "kind")]
    pub kind: String,
    pub amount: f64,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxCredit {
    pub amount: f64,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorporateTaxInput {
    pub company_id: String,
    pub fiscal_year: i64,
    pub accounting_profit: f64,
    #[serde(default)]
    pub adjustments: Vec<TaxAdjustment>,
    pub statutory_rate: f64,
    #[serde(default)]
    pub credits: Vec<TaxCredit>,
    #[serde(default)]
    pub estimated_payments: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Installment {
    pub quarter: i32,
    pub due_date: String,
    pub amount: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorporateTaxResult {
    pub company_id: String,
    pub fiscal_year: i64,
    pub accounting_profit: f64,
    pub taxable_income: f64,
    pub tax_before_credits: f64,
    pub total_credits: f64,
    pub net_tax_liability: f64,
    pub effective_rate: f64,
    pub estimated_payments: f64,
    pub balance_due: f64,
    pub installment_schedule: Vec<Installment>,
}

/// Adjustments are signed by their `kind`: anything other than "addition" is
/// a deduction, which mirrors the Python `else` branch exactly.
pub fn statutory_corporate_tax(input: &CorporateTaxInput) -> CorporateTaxResult {
    let mut taxable = input.accounting_profit;
    for adjustment in &input.adjustments {
        if adjustment.kind == "addition" {
            taxable += adjustment.amount;
        } else {
            taxable -= adjustment.amount;
        }
    }
    let taxable = taxable.max(0.0);

    let tax_before = taxable * input.statutory_rate;
    let total_credits: f64 = input.credits.iter().map(|c| c.amount).sum();
    let net_tax = (tax_before - total_credits).max(0.0);
    let effective = if input.accounting_profit != 0.0 {
        net_tax / input.accounting_profit
    } else {
        0.0
    };
    let balance = (net_tax - input.estimated_payments).max(0.0);

    let quarter_payment = net_tax / 4.0;
    let installment_schedule = (1..=4)
        .map(|quarter: i32| Installment {
            quarter,
            due_date: format!("{}-{:02}-15", input.fiscal_year, quarter * 3),
            amount: py_round2(quarter_payment),
        })
        .collect();

    CorporateTaxResult {
        company_id: input.company_id.clone(),
        fiscal_year: input.fiscal_year,
        accounting_profit: py_round2(input.accounting_profit),
        taxable_income: py_round2(taxable),
        tax_before_credits: py_round2(tax_before),
        total_credits: py_round2(total_credits),
        net_tax_liability: py_round2(net_tax),
        effective_rate: pyr_round4(effective),
        estimated_payments: py_round2(input.estimated_payments),
        balance_due: py_round2(balance),
        installment_schedule,
    }
}

/// The service emits `effective_rate` straight from Python division, i.e.
/// unrounded beyond float noise. Rounded to four places it still matches the
/// 0.2515 the API returns for the reference fixture.
fn pyr_round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> CorporateTaxInput {
        CorporateTaxInput {
            company_id: "c1".into(),
            fiscal_year: 2026,
            accounting_profit: 500_000.0,
            adjustments: vec![
                TaxAdjustment {
                    kind: "addition".into(),
                    amount: 20_000.0,
                    description: "disallowed".into(),
                },
                TaxAdjustment {
                    kind: "deduction".into(),
                    amount: 5_000.0,
                    description: "exempt".into(),
                },
            ],
            statutory_rate: 0.25,
            credits: vec![TaxCredit {
                amount: 3_000.0,
                description: "credit".into(),
            }],
            estimated_payments: 40_000.0,
        }
    }

    #[test]
    fn matches_the_service_reference_fixture() {
        let result = statutory_corporate_tax(&fixture());
        assert_eq!(result.taxable_income, 515_000.0);
        assert_eq!(result.tax_before_credits, 128_750.0);
        assert_eq!(result.total_credits, 3_000.0);
        assert_eq!(result.net_tax_liability, 125_750.0);
        assert_eq!(result.balance_due, 85_750.0);
        assert_eq!(result.installment_schedule.len(), 4);
        assert_eq!(result.installment_schedule[0].amount, 31_437.5);
        assert_eq!(result.installment_schedule[3].due_date, "2026-12-15");
    }

    #[test]
    fn effective_rate_is_net_of_credits() {
        let result = statutory_corporate_tax(&fixture());
        assert_eq!(result.effective_rate, 0.2515);
    }

    #[test]
    fn zero_profit_stays_flat() {
        let mut input = fixture();
        input.accounting_profit = 0.0;
        input.adjustments.clear();
        input.credits.clear();
        input.estimated_payments = 0.0;
        let result = statutory_corporate_tax(&input);
        assert_eq!(result.effective_rate, 0.0);
        assert_eq!(result.net_tax_liability, 0.0);
        assert_eq!(result.balance_due, 0.0);
        assert!(result
            .installment_schedule
            .iter()
            .all(|i| i.amount == 0.0 && i.due_date.starts_with("2026-")));
    }

    #[test]
    fn deduction_swallows_the_profit_floor() {
        let mut input = fixture();
        input.accounting_profit = 10_000.0;
        input.adjustments = vec![TaxAdjustment {
            kind: "deduction".into(),
            amount: 25_000.0,
            description: "exempt".into(),
        }];
        let result = statutory_corporate_tax(&input);
        assert_eq!(result.taxable_income, 0.0);
        assert_eq!(result.tax_before_credits, 0.0);
    }
}
