//! tax-calculation-service: progressive brackets, VAT, withholding,
//! plus tax-provision-service's deferred-tax provision.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vimbai_common::py_round2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxType {
    Corporate,
    Vat,
    Withholding,
    CapitalGains,
    Paye,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxBracket {
    pub min_amount: f64,
    pub max_amount: Option<f64>,
    pub rate: f64,
}

/// Zimbabwe corporate brackets (2026 fixture from the Python service).
pub fn default_zw_brackets() -> Vec<TaxBracket> {
    vec![
        TaxBracket {
            min_amount: 0.0,
            max_amount: Some(12500.0),
            rate: 0.0,
        },
        TaxBracket {
            min_amount: 12500.0,
            max_amount: Some(25000.0),
            rate: 0.20,
        },
        TaxBracket {
            min_amount: 25000.0,
            max_amount: Some(75000.0),
            rate: 0.25,
        },
        TaxBracket {
            min_amount: 75000.0,
            max_amount: None,
            rate: 0.35,
        },
    ]
}

/// Python returns `(tax_owed, breakdown, marginal_rate)`; the breakdown
/// key format is `bracket_<min>_<rate>` exactly as the service emits it.
pub fn calculate_progressive_tax(
    amount: f64,
    brackets: &[TaxBracket],
) -> (f64, BTreeMap<String, f64>, f64) {
    let mut tax_owed = 0.0;
    let mut breakdown = BTreeMap::new();
    let mut marginal_rate = 0.0;
    let mut remaining = amount.max(0.0);

    for b in brackets {
        if remaining <= 0.0 {
            break;
        }
        let bracket_max = b.max_amount.unwrap_or(f64::INFINITY);
        let taxable_in_bracket = remaining.min(bracket_max - b.min_amount);
        if taxable_in_bracket > 0.0 {
            let bracket_tax = taxable_in_bracket * b.rate;
            tax_owed += bracket_tax;
            breakdown.insert(
                format!("bracket_{}_{}", fmt_num(b.min_amount), fmt_num(b.rate)),
                bracket_tax,
            );
            marginal_rate = b.rate;
            remaining -= taxable_in_bracket;
        }
    }
    (tax_owed, breakdown, marginal_rate)
}

/// Python's f-string rendering of the numbers used in breakdown keys
/// (`0` -> "0", `12500.0` -> "12500.0", `0.2` -> "0.2").
fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{:.1}", v)
    } else {
        format!("{}", v)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxCalculation {
    pub company_id: String,
    pub tax_type: TaxType,
    pub taxable_amount: f64,
    pub deductions: f64,
    pub tax_owed: f64,
    pub effective_rate: f64,
    pub marginal_rate: f64,
    pub breakdown: BTreeMap<String, f64>,
    pub fiscal_year: i64,
}

#[allow(clippy::too_many_arguments)]
/// `country` only matters for withholding (ZW 10%, else 15%).
pub fn calculate_tax(
    company_id: &str,
    tax_type: TaxType,
    country: &str,
    taxable_amount: f64,
    deductions: f64,
    brackets: Option<&[TaxBracket]>,
    fiscal_year: i64,
) -> TaxCalculation {
    let brackets_storage;
    let brackets = match brackets {
        Some(b) => b,
        None => {
            brackets_storage = default_zw_brackets();
            &brackets_storage[..]
        }
    };
    let net_taxable = (taxable_amount - deductions).max(0.0);

    let (tax_owed, effective_rate, marginal_rate, breakdown) = match tax_type {
        TaxType::Vat => {
            let owed = net_taxable * 0.15;
            let mut bd = BTreeMap::new();
            bd.insert("vat_standard".to_string(), owed);
            (owed, 0.15, 0.15, bd)
        }
        TaxType::Withholding => {
            let rate = if country == "ZW" { 0.10 } else { 0.15 };
            let owed = net_taxable * rate;
            let mut bd = BTreeMap::new();
            bd.insert("withholding".to_string(), owed);
            (owed, rate, rate, bd)
        }
        _ => {
            let (owed, bd, marginal) = calculate_progressive_tax(net_taxable, brackets);
            let eff = if taxable_amount > 0.0 {
                owed / taxable_amount
            } else {
                0.0
            };
            (owed, eff, marginal, bd)
        }
    };

    TaxCalculation {
        company_id: company_id.to_string(),
        tax_type,
        taxable_amount,
        deductions,
        tax_owed: py_round2(tax_owed),
        effective_rate: if tax_type == TaxType::Vat || tax_type == TaxType::Withholding {
            effective_rate
        } else {
            // progressive branch rounds effective rate to 4dp in Python
            py_round4(effective_rate)
        },
        marginal_rate,
        breakdown,
        fiscal_year,
    }
}

fn py_round4(v: f64) -> f64 {
    format!("{:.4}", v).parse().unwrap_or(v)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempDifferenceType {
    Taxable,
    Deductible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempDifference {
    pub book_amount: f64,
    pub tax_amount: f64,
    #[serde(rename = "type")]
    pub kind: TempDifferenceType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxProvisionRequest {
    pub company_id: String,
    pub fiscal_year: i64,
    pub pre_tax_income: f64,
    #[serde(default)]
    pub permanent_differences: f64,
    #[serde(default)]
    pub temp_differences: Vec<TempDifference>,
    #[serde(default)]
    pub loss_carryforward: f64,
    pub statutory_rate: f64,
    #[serde(default)]
    pub valuation_allowance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxProvisionResult {
    pub company_id: String,
    pub fiscal_year: i64,
    pub current_tax_expense: f64,
    pub deferred_tax_expense: f64,
    pub total_tax_expense: f64,
    pub effective_rate: f64,
    pub deferred_tax_assets: f64,
    pub deferred_tax_liabilities: f64,
    pub net_deferred_tax: f64,
    pub valuation_allowance: f64,
    pub loss_carryforward_utilized: f64,
    pub statutory_rate: f64,
}

/// tax-provision-service `/calculate` (deferred tax / IAS 12 shape).
pub fn tax_provision(req: &TaxProvisionRequest) -> TaxProvisionResult {
    let mut taxable_income = req.pre_tax_income + req.permanent_differences;
    for td in &req.temp_differences {
        let diff = td.book_amount - td.tax_amount;
        match td.kind {
            TempDifferenceType::Taxable => taxable_income += diff,
            TempDifferenceType::Deductible => taxable_income -= diff,
        }
    }

    let utilized = if req.loss_carryforward > 0.0 {
        let u = req.loss_carryforward.min(taxable_income.max(0.0));
        taxable_income -= u;
        u
    } else {
        0.0
    };

    let current_tax = taxable_income.max(0.0) * req.statutory_rate;

    let mut dta = 0.0;
    let mut dtl = 0.0;
    for td in &req.temp_differences {
        let diff = (td.book_amount - td.tax_amount).abs();
        match td.kind {
            TempDifferenceType::Deductible => dta += diff * req.statutory_rate,
            TempDifferenceType::Taxable => dtl += diff * req.statutory_rate,
        }
    }
    let net_deferred = dta - dtl - req.valuation_allowance;
    let deferred_tax = dtl - dta;
    let total_tax = current_tax + deferred_tax;
    let effective_rate = if req.pre_tax_income != 0.0 {
        total_tax / req.pre_tax_income
    } else {
        0.0
    };

    TaxProvisionResult {
        company_id: req.company_id.clone(),
        fiscal_year: req.fiscal_year,
        current_tax_expense: py_round2(current_tax),
        deferred_tax_expense: py_round2(deferred_tax),
        total_tax_expense: py_round2(total_tax),
        effective_rate: py_round4(effective_rate),
        deferred_tax_assets: py_round2(dta),
        deferred_tax_liabilities: py_round2(dtl),
        net_deferred_tax: py_round2(net_deferred),
        valuation_allowance: req.valuation_allowance,
        loss_carryforward_utilized: py_round2(utilized),
        statutory_rate: req.statutory_rate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progressive_zw_brackets() {
        // 100_000 taxable: 12_500 free, 12_500*0.20=2_500,
        // 50_000*0.25=12_500, 25_000*0.35=8_750 -> 23_750
        let (owed, bd, marginal) = calculate_progressive_tax(100_000.0, &default_zw_brackets());
        assert_eq!(owed, 23_750.0);
        assert_eq!(marginal, 0.35);
        assert_eq!(bd.len(), 4);
        assert_eq!(bd["bracket_75000.0_0.35"], 8_750.0);
    }

    #[test]
    fn progressive_mid_bracket() {
        let (owed, _bd, marginal) = calculate_progressive_tax(20_000.0, &default_zw_brackets());
        assert_eq!(owed, 1_500.0); // 7_500 * 0.20
        assert_eq!(marginal, 0.20);
    }

    #[test]
    fn zero_income_stays_zero() {
        let (owed, bd, marginal) = calculate_progressive_tax(0.0, &default_zw_brackets());
        assert_eq!(owed, 0.0);
        assert!(bd.is_empty());
        assert_eq!(marginal, 0.0);
    }

    #[test]
    fn vat_flat_15() {
        let r = calculate_tax("c1", TaxType::Vat, "ZW", 10_000.0, 0.0, None, 2026);
        assert_eq!(r.tax_owed, 1_500.0);
        assert_eq!(r.effective_rate, 0.15);
        assert_eq!(r.breakdown["vat_standard"], 1_500.0);
    }

    #[test]
    fn withholding_country_rates() {
        let zw = calculate_tax("c1", TaxType::Withholding, "ZW", 10_000.0, 0.0, None, 2026);
        assert_eq!(zw.tax_owed, 1_000.0);
        let other = calculate_tax("c1", TaxType::Withholding, "ZA", 10_000.0, 0.0, None, 2026);
        assert_eq!(other.tax_owed, 1_500.0);
    }

    #[test]
    fn corporate_deductions_floor_at_zero() {
        let r = calculate_tax("c1", TaxType::Corporate, "ZW", 5_000.0, 9_000.0, None, 2026);
        assert_eq!(r.tax_owed, 0.0);
        assert_eq!(r.effective_rate, 0.0);
    }

    #[test]
    fn breakdown_key_matches_python_fstring() {
        let (_, bd, _) = calculate_progressive_tax(30_000.0, &default_zw_brackets());
        assert!(
            bd.contains_key("bracket_0.0_0.0"),
            "keys: {:?}",
            bd.keys().collect::<Vec<_>>()
        );
        assert!(bd.contains_key("bracket_12500.0_0.2"));
    }

    fn provision_req() -> TaxProvisionRequest {
        TaxProvisionRequest {
            company_id: "c1".into(),
            fiscal_year: 2026,
            pre_tax_income: 500_000.0,
            permanent_differences: 20_000.0,
            temp_differences: vec![
                TempDifference {
                    book_amount: 150_000.0,
                    tax_amount: 100_000.0,
                    kind: TempDifferenceType::Taxable,
                },
                TempDifference {
                    book_amount: 80_000.0,
                    tax_amount: 100_000.0,
                    kind: TempDifferenceType::Deductible,
                },
            ],
            loss_carryforward: 0.0,
            statutory_rate: 0.25,
            valuation_allowance: 0.0,
        }
    }

    #[test]
    fn provision_deferred_tax() {
        let r = tax_provision(&provision_req());
        // Python: 520_000 +50_000, then -= (80_000-100_000) -> 590_000
        // (the deductible branch subtracts a negative diff, i.e. adds)
        assert_eq!(r.current_tax_expense, 147_500.0);
        // dtl 50_000*0.25=12_500, dta 20_000*0.25=5_000, deferred = 7_500
        assert_eq!(r.deferred_tax_assets, 5_000.0);
        assert_eq!(r.deferred_tax_liabilities, 12_500.0);
        assert_eq!(r.deferred_tax_expense, 7_500.0);
        assert_eq!(r.total_tax_expense, 155_000.0);
        assert_eq!(r.effective_rate, 0.31);
        // net = dta - dtl - allowance = 5_000 - 12_500
        assert_eq!(r.net_deferred_tax, -7_500.0);
    }

    #[test]
    fn provision_loss_carryforward() {
        let mut req = provision_req();
        req.pre_tax_income = 100_000.0;
        req.permanent_differences = 0.0;
        req.temp_differences = vec![];
        req.loss_carryforward = 40_000.0;
        let r = tax_provision(&req);
        assert_eq!(r.loss_carryforward_utilized, 40_000.0);
        // 100_000 - 40_000 = 60_000 * 0.25 = 15_000
        assert_eq!(r.current_tax_expense, 15_000.0);
    }

    #[test]
    fn provision_valuation_allowance_reduces_net_deferred() {
        let mut req = provision_req();
        req.valuation_allowance = 3_000.0;
        let r = tax_provision(&req);
        assert_eq!(r.net_deferred_tax, -10_500.0);
        assert_eq!(
            r.total_tax_expense, 155_000.0,
            "allowance does not hit P&L tax here"
        );
    }
}
