//! balance-sheet-service: server-side totals, balance check and the
//! ratio endpoint (every denominator floored at 1).
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BSItem {
    pub amount: f64,
    /// "current" or "non_current".
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub is_liquid: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BalanceSheetInput {
    pub company_id: String,
    pub assets: Vec<BSItem>,
    pub liabilities: Vec<BSItem>,
    pub equity: Vec<BSItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BalanceSheetTotals {
    pub company_id: String,
    pub total_assets: f64,
    pub total_liabilities: f64,
    pub total_equity: f64,
    pub is_balanced: bool,
}

impl Default for BSItem {
    fn default() -> Self {
        BSItem {
            amount: 0.0,
            category: String::new(),
            is_liquid: false,
        }
    }
}

/// Totals are always recomputed server side; `is_balanced` allows a
/// one-cent tolerance, exactly as the Python model does.
pub fn generate_totals(input: &BalanceSheetInput) -> BalanceSheetTotals {
    let total_assets: f64 = input.assets.iter().map(|a| a.amount).sum();
    let total_liabilities: f64 = input.liabilities.iter().map(|l| l.amount).sum();
    let total_equity: f64 = input.equity.iter().map(|e| e.amount).sum();
    BalanceSheetTotals {
        company_id: input.company_id.clone(),
        total_assets,
        total_liabilities,
        total_equity,
        is_balanced: (total_assets - (total_liabilities + total_equity)).abs() < 0.01,
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BalanceSheetRatios {
    pub current_ratio: f64,
    pub quick_ratio: f64,
    pub debt_to_equity: f64,
    pub debt_to_assets: f64,
    pub equity_ratio: f64,
    pub working_capital: f64,
}

pub fn balance_sheet_ratios(input: &BalanceSheetInput) -> BalanceSheetRatios {
    let current_assets: f64 = input
        .assets
        .iter()
        .filter(|a| a.category == "current")
        .map(|a| a.amount)
        .sum();
    let current_liabilities: f64 = input
        .liabilities
        .iter()
        .filter(|l| l.category == "current")
        .map(|l| l.amount)
        .sum();
    let liquid_assets: f64 = input
        .assets
        .iter()
        .filter(|a| a.is_liquid)
        .map(|a| a.amount)
        .sum();
    let t = generate_totals(input);
    let denom = current_liabilities.max(1.0);
    BalanceSheetRatios {
        current_ratio: py_round2(current_assets / denom),
        quick_ratio: py_round2(liquid_assets / denom),
        debt_to_equity: py_round2(t.total_liabilities / t.total_equity.max(1.0)),
        debt_to_assets: py_round2(t.total_liabilities / t.total_assets.max(1.0)),
        equity_ratio: py_round2(t.total_equity / t.total_assets.max(1.0)),
        working_capital: py_round2(current_assets - current_liabilities),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(amount: f64, category: &str, liquid: bool) -> BSItem {
        BSItem {
            amount,
            category: category.into(),
            is_liquid: liquid,
        }
    }

    fn sheet() -> BalanceSheetInput {
        BalanceSheetInput {
            company_id: "c1".into(),
            assets: vec![
                item(200_000.0, "current", true),
                item(800_000.0, "non_current", false),
            ],
            liabilities: vec![
                item(150_000.0, "current", false),
                item(350_000.0, "non_current", false),
            ],
            equity: vec![item(500_000.0, "", false)],
        }
    }

    #[test]
    fn totals_and_balance_check() {
        let t = generate_totals(&sheet());
        assert_eq!(t.total_assets, 1_000_000.0);
        assert_eq!(t.total_liabilities, 500_000.0);
        assert_eq!(t.total_equity, 500_000.0);
        assert!(t.is_balanced);
    }

    #[test]
    fn imbalance_beyond_a_cent_is_flagged() {
        let mut s = sheet();
        s.equity = vec![item(499_980.0, "", false)];
        assert!(!generate_totals(&s).is_balanced);
        // a true one-cent gap is NOT tolerated: the check is strictly < 0.01
        s.equity = vec![item(499_999.99, "", false)];
        assert!(!generate_totals(&s).is_balanced);
        // a sub-cent float gap passes
        s.equity = vec![item(499_999.995, "", false)];
        assert!(generate_totals(&s).is_balanced);
    }

    #[test]
    fn ratios_floor_denominators_at_one() {
        let r = balance_sheet_ratios(&sheet());
        assert_eq!(r.current_ratio, 1.33);
        assert_eq!(r.quick_ratio, 1.33);
        assert_eq!(r.debt_to_equity, 1.0);
        assert_eq!(r.debt_to_assets, 0.5);
        assert_eq!(r.equity_ratio, 0.5);
        assert_eq!(r.working_capital, 50_000.0);

        let empty = balance_sheet_ratios(&BalanceSheetInput::default());
        assert_eq!(empty.current_ratio, 0.0);
        assert_eq!(empty.debt_to_equity, 0.0);
    }
}
