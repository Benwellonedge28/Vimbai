//! r-and-d-tax-service `/calculate`: regular vs simplified credit.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expenditure {
    pub category: String,
    pub amount: f64,
    #[serde(default)]
    pub qualifies: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdClaimResult {
    pub company_id: String,
    pub fiscal_year: i64,
    pub total_expenditure: f64,
    pub qualifying_expenditure: f64,
    pub non_qualifying: f64,
    pub credit_regular: f64,
    pub credit_alternative: f64,
    pub recommended_method: String,
    pub expenditure_breakdown: BTreeMap<String, f64>,
}

pub fn rd_credit_claim(
    company_id: &str,
    fiscal_year: i64,
    expenditures: &[Expenditure],
    credit_rate: f64,
    alternative_rate: f64,
) -> RdClaimResult {
    let total: f64 = expenditures.iter().map(|e| e.amount).sum();
    let qualifying: f64 = expenditures
        .iter()
        .filter(|e| e.qualifies)
        .map(|e| e.amount)
        .sum();
    let non_qualifying = total - qualifying;

    let credit_reg = qualifying * credit_rate;
    let credit_alt = qualifying * alternative_rate;

    let mut breakdown: BTreeMap<String, f64> = BTreeMap::new();
    for e in expenditures.iter().filter(|e| e.qualifies) {
        *breakdown.entry(e.category.clone()).or_insert(0.0) += e.amount;
    }

    // Ties go to regular (Python compares `>`).
    let recommended = if credit_reg > credit_alt {
        "regular"
    } else {
        "alternative_simplified"
    };

    RdClaimResult {
        company_id: company_id.to_string(),
        fiscal_year,
        total_expenditure: py_round2(total),
        qualifying_expenditure: py_round2(qualifying),
        non_qualifying: py_round2(non_qualifying),
        credit_regular: py_round2(credit_reg),
        credit_alternative: py_round2(credit_alt),
        recommended_method: recommended.to_string(),
        expenditure_breakdown: breakdown
            .into_iter()
            .map(|(k, v)| (k, py_round2(v)))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exp() -> Vec<Expenditure> {
        vec![
            Expenditure {
                category: "salaries".into(),
                amount: 100_000.0,
                qualifies: true,
            },
            Expenditure {
                category: "salaries".into(),
                amount: 50_000.0,
                qualifies: true,
            },
            Expenditure {
                category: "equipment".into(),
                amount: 80_000.0,
                qualifies: true,
            },
            Expenditure {
                category: "marketing".into(),
                amount: 20_000.0,
                qualifies: false,
            },
        ]
    }

    #[test]
    fn qualifying_split_and_breakdown() {
        let r = rd_credit_claim("c1", 2026, &exp(), 0.20, 0.14);
        assert_eq!(r.total_expenditure, 250_000.0);
        assert_eq!(r.qualifying_expenditure, 230_000.0);
        assert_eq!(r.non_qualifying, 20_000.0);
        assert_eq!(r.expenditure_breakdown["salaries"], 150_000.0);
        assert_eq!(r.expenditure_breakdown["equipment"], 80_000.0);
        assert!(!r.expenditure_breakdown.contains_key("marketing"));
    }

    #[test]
    fn regular_credit_wins() {
        let r = rd_credit_claim("c1", 2026, &exp(), 0.20, 0.14);
        assert_eq!(r.credit_regular, 46_000.0);
        assert_eq!(r.credit_alternative, 32_200.0);
        assert_eq!(r.recommended_method, "regular");
    }

    #[test]
    fn tie_goes_to_alternative() {
        // Python: regular only when strictly greater; tie -> alternative
        let r = rd_credit_claim("c1", 2026, &exp(), 0.15, 0.15);
        assert_eq!(r.recommended_method, "alternative_simplified");
    }
}
