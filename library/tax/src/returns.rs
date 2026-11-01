//! tax-return-preparation-service `/prepare`: 25% flat corporate rate
//! with permanent adjustments and credits.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vimbai_common::py_round2;

fn py_round4(v: f64) -> f64 {
    format!("{:.4}", v).parse().unwrap_or(v)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxComponent {
    pub component_name: String,
    pub taxable_amount: f64,
    pub tax_rate: f64,
    pub tax_amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxReturnRequest {
    pub company_id: String,
    pub fiscal_year: i64,
    pub accounting_profit: f64,
    #[serde(default)]
    pub permanent_differences: Vec<BTreeMap<String, f64>>,
    #[serde(default)]
    pub temporary_differences: Vec<BTreeMap<String, f64>>,
    #[serde(default)]
    pub tax_credits: Vec<BTreeMap<String, f64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxReturnResult {
    pub company_id: String,
    pub fiscal_year: i64,
    pub tax_components: Vec<TaxComponent>,
    pub total_taxable_income: f64,
    pub total_tax_payable: f64,
    pub effective_tax_rate: f64,
    pub deferred_tax_assets: f64,
    pub deferred_tax_liabilities: f64,
}

/// `deferred_tax_assets` sums the odd "dtl_assets" key and
/// `deferred_tax_liabilities` sums "dtl_liabilities", exactly as the
/// service does (the key names are inherited from its data shape).
pub fn prepare_tax_return(req: &TaxReturnRequest) -> TaxReturnResult {
    let mut taxable_income = req.accounting_profit;
    for perm in &req.permanent_differences {
        taxable_income += perm.get("adjustment").copied().unwrap_or(0.0);
    }

    let dta: f64 = req
        .temporary_differences
        .iter()
        .map(|td| td.get("dtl_assets").copied().unwrap_or(0.0))
        .sum();
    let dtl: f64 = req
        .temporary_differences
        .iter()
        .map(|td| td.get("dtl_liabilities").copied().unwrap_or(0.0))
        .sum();

    let tax_rate = 0.25;
    let total_tax = taxable_income * tax_rate;
    let credits: f64 = req
        .tax_credits
        .iter()
        .map(|tc| tc.get("credit_amount").copied().unwrap_or(0.0))
        .sum();
    let net_tax = (total_tax - credits).max(0.0);
    let effective_rate = if req.accounting_profit != 0.0 {
        net_tax / req.accounting_profit
    } else {
        0.0
    };

    TaxReturnResult {
        company_id: req.company_id.clone(),
        fiscal_year: req.fiscal_year,
        tax_components: vec![
            TaxComponent {
                component_name: "Corporate Income Tax".into(),
                taxable_amount: taxable_income,
                tax_rate,
                tax_amount: total_tax,
            },
            TaxComponent {
                component_name: "Tax Credits".into(),
                taxable_amount: 0.0,
                tax_rate: 0.0,
                tax_amount: -credits,
            },
        ],
        total_taxable_income: py_round2(taxable_income),
        total_tax_payable: py_round2(net_tax),
        effective_tax_rate: py_round4(effective_rate),
        deferred_tax_assets: py_round2(dta),
        deferred_tax_liabilities: py_round2(dtl),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> TaxReturnRequest {
        let mut perm = BTreeMap::new();
        perm.insert("adjustment".to_string(), 30_000.0);
        let mut td_a = BTreeMap::new();
        td_a.insert("dtl_assets".to_string(), 12_000.0);
        let mut td_l = BTreeMap::new();
        td_l.insert("dtl_liabilities".to_string(), 25_000.0);
        let mut credit = BTreeMap::new();
        credit.insert("credit_amount".to_string(), 40_000.0);
        TaxReturnRequest {
            company_id: "c1".into(),
            fiscal_year: 2026,
            accounting_profit: 400_000.0,
            permanent_differences: vec![perm],
            temporary_differences: vec![td_a, td_l],
            tax_credits: vec![credit],
        }
    }

    #[test]
    fn taxable_income_and_payable() {
        let r = prepare_tax_return(&req());
        // 400_000 + 30_000 = 430_000 * 0.25 = 107_500 - 40_000 credits = 67_500
        assert_eq!(r.total_taxable_income, 430_000.0);
        assert_eq!(r.total_tax_payable, 67_500.0);
        assert_eq!(r.effective_tax_rate, 0.1688);
        assert_eq!(r.tax_components[1].tax_amount, -40_000.0);
    }

    #[test]
    fn deferred_balances_from_odd_keys() {
        let r = prepare_tax_return(&req());
        assert_eq!(r.deferred_tax_assets, 12_000.0);
        assert_eq!(r.deferred_tax_liabilities, 25_000.0);
    }

    #[test]
    fn credits_cannot_push_below_zero() {
        let mut q = req();
        q.tax_credits = vec![{
            let mut m = BTreeMap::new();
            m.insert("credit_amount".to_string(), 500_000.0);
            m
        }];
        q.permanent_differences = vec![];
        let r = prepare_tax_return(&q);
        assert_eq!(r.total_tax_payable, 0.0);
    }
}
