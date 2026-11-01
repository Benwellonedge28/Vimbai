//! group-tax-service `/consolidate`: group consolidation with loss
//! offsetting and the service's savings formula.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsidiaryTax {
    pub entity_id: String,
    pub entity_name: String,
    pub pre_tax_income: f64,
    pub tax_paid: f64,
    pub tax_rate: f64,
    #[serde(default)]
    pub loss_carryforward: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupTaxRequest {
    pub group_id: String,
    pub fiscal_year: i64,
    pub subsidiaries: Vec<SubsidiaryTax>,
    #[serde(default = "default_group_rate")]
    pub group_tax_rate: f64,
    #[serde(default)]
    pub intercompany_eliminations: f64,
}

fn default_group_rate() -> f64 {
    0.25
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsidiarySummary {
    pub entity_id: String,
    pub name: String,
    pub pre_tax_income: f64,
    pub tax_paid: f64,
    pub rate: f64,
    pub loss_carryforward: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupTaxResult {
    pub group_id: String,
    pub fiscal_year: i64,
    pub consolidated_income: f64,
    pub consolidated_tax: f64,
    pub total_tax_already_paid: f64,
    pub net_group_tax: f64,
    pub tax_savings_from_consolidation: f64,
    pub subsidiary_summary: Vec<SubsidiarySummary>,
}

pub fn consolidate_group_tax(req: &GroupTaxRequest) -> GroupTaxResult {
    let total_income = req
        .subsidiaries
        .iter()
        .map(|s| s.pre_tax_income)
        .sum::<f64>()
        - req.intercompany_eliminations;
    let total_tax_paid: f64 = req.subsidiaries.iter().map(|s| s.tax_paid).sum();

    let consolidated_income = total_income.max(0.0);
    let consolidated_tax = consolidated_income * req.group_tax_rate;

    let standalone_tax: f64 = req
        .subsidiaries
        .iter()
        .map(|s| s.pre_tax_income.max(0.0) * s.tax_rate)
        .sum();

    let overpaid: f64 = req
        .subsidiaries
        .iter()
        .filter(|s| s.pre_tax_income > 0.0)
        .map(|s| s.pre_tax_income.max(0.0) * s.tax_rate - s.tax_paid)
        .sum();

    // Faithful to the service: savings = standalone - consolidated - overpaid,
    // floored at zero (the formula double-counts by design in the original).
    let savings = (standalone_tax - consolidated_tax - overpaid).max(0.0);

    let summary = req
        .subsidiaries
        .iter()
        .map(|s| SubsidiarySummary {
            entity_id: s.entity_id.clone(),
            name: s.entity_name.clone(),
            pre_tax_income: s.pre_tax_income,
            tax_paid: s.tax_paid,
            rate: s.tax_rate,
            loss_carryforward: s.loss_carryforward,
        })
        .collect();

    GroupTaxResult {
        group_id: req.group_id.clone(),
        fiscal_year: req.fiscal_year,
        consolidated_income: py_round2(consolidated_income),
        consolidated_tax: py_round2(consolidated_tax),
        total_tax_already_paid: py_round2(total_tax_paid),
        net_group_tax: py_round2((consolidated_tax - total_tax_paid).max(0.0)),
        tax_savings_from_consolidation: py_round2(savings),
        subsidiary_summary: summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> GroupTaxRequest {
        GroupTaxRequest {
            group_id: "g1".into(),
            fiscal_year: 2026,
            group_tax_rate: 0.25,
            intercompany_eliminations: 10_000.0,
            subsidiaries: vec![
                SubsidiaryTax {
                    entity_id: "e1".into(),
                    entity_name: "Alpha".into(),
                    pre_tax_income: 200_000.0,
                    tax_paid: 50_000.0,
                    tax_rate: 0.25,
                    loss_carryforward: 0.0,
                },
                SubsidiaryTax {
                    entity_id: "e2".into(),
                    entity_name: "Beta".into(),
                    pre_tax_income: -50_000.0,
                    tax_paid: 0.0,
                    tax_rate: 0.30,
                    loss_carryforward: 50_000.0,
                },
            ],
        }
    }

    #[test]
    fn consolidation_offsets_losses() {
        let r = consolidate_group_tax(&req());
        // total income 200k - 50k - 10k elim = 140k; consolidated tax 35k
        assert_eq!(r.consolidated_income, 140_000.0);
        assert_eq!(r.consolidated_tax, 35_000.0);
        assert_eq!(r.total_tax_already_paid, 50_000.0);
        // net group tax floored at zero since prepaid exceeds liability
        assert_eq!(r.net_group_tax, 0.0);
        assert_eq!(r.subsidiary_summary.len(), 2);
        assert_eq!(r.subsidiary_summary[1].loss_carryforward, 50_000.0);
    }

    #[test]
    fn savings_never_negative() {
        let r = consolidate_group_tax(&req());
        assert!(r.tax_savings_from_consolidation >= 0.0);
    }

    #[test]
    fn all_losses_means_zero_consolidated_income() {
        let mut q = req();
        q.subsidiaries[0].pre_tax_income = -100_000.0;
        let r = consolidate_group_tax(&q);
        assert_eq!(r.consolidated_income, 0.0);
        assert_eq!(r.consolidated_tax, 0.0);
    }
}
