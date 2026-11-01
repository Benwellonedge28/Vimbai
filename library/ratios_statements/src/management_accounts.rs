//! management-accounts-service `/prepare`: P&L, balance sheet, working
//! capital and key ratios from caller-supplied line-item maps.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

/// Line items arrive as ordered pairs so the caller's ordering (and thus
/// the Python dict order) is preserved; lookups fall back like `.get()`.
pub type Items = Vec<(String, f64)>;

fn get(items: &Items, key: &str) -> f64 {
    items
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| *v)
        .unwrap_or(0.0)
}

fn get_or(items: &Items, key: &str, default: f64) -> f64 {
    items
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| *v)
        .unwrap_or(default)
}

fn total(items: &Items) -> f64 {
    items.iter().map(|(_, v)| *v).sum()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PnlSummary {
    pub revenue: f64,
    pub cogs: f64,
    pub gross_profit: f64,
    pub opex: f64,
    pub ebitda: f64,
    pub gross_margin: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BalanceSummary {
    pub total_assets: f64,
    pub total_liabilities: f64,
    pub net_assets: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkingCapital {
    pub working_capital: f64,
    pub current_ratio: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyRatios {
    pub gross_margin_pct: f64,
    pub ebitda_margin_pct: f64,
    pub current_ratio: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManagementAccounts {
    pub company_id: String,
    pub period: String,
    pub pnl_statement: PnlSummary,
    pub balance_sheet: BalanceSummary,
    pub working_capital: WorkingCapital,
    pub key_ratios: KeyRatios,
}

/// PYTHON QUIRK: `current_liabilities` defaults to 1, not 0, for the
/// working-capital block, so a caller who omits it gets capital minus one
/// and a current ratio equal to current assets.
pub fn prepare_management_accounts(
    company_id: &str,
    period: &str,
    revenue_items: &Items,
    cost_items: &Items,
    balance_sheet_items: &Items,
) -> ManagementAccounts {
    let total_revenue = total(revenue_items);
    let total_costs = total(cost_items);
    let ebitda = total_revenue - total_costs;
    let cogs = get(cost_items, "cogs");
    let opex = get(cost_items, "opex");
    let gross_profit = total_revenue - cogs;
    let gross_margin = if total_revenue == 0.0 {
        0.0
    } else {
        gross_profit / total_revenue * 100.0
    };

    let total_assets =
        get(balance_sheet_items, "current_assets") + get(balance_sheet_items, "non_current_assets");
    let total_liabilities = get(balance_sheet_items, "current_liabilities")
        + get(balance_sheet_items, "non_current_liabilities");

    let current_assets = get(balance_sheet_items, "current_assets");
    let current_liabilities = get_or(balance_sheet_items, "current_liabilities", 1.0);
    let current_ratio = if current_liabilities == 0.0 {
        0.0
    } else {
        current_assets / current_liabilities
    };

    ManagementAccounts {
        company_id: company_id.to_string(),
        period: period.to_string(),
        pnl_statement: PnlSummary {
            revenue: py_round2(total_revenue),
            cogs: py_round2(cogs),
            gross_profit: py_round2(gross_profit),
            opex: py_round2(opex),
            ebitda: py_round2(ebitda),
            gross_margin: py_round2(gross_margin),
        },
        balance_sheet: BalanceSummary {
            total_assets: py_round2(total_assets),
            total_liabilities: py_round2(total_liabilities),
            net_assets: py_round2(total_assets - total_liabilities),
        },
        working_capital: WorkingCapital {
            working_capital: py_round2(current_assets - current_liabilities),
            current_ratio: py_round2(current_ratio),
        },
        key_ratios: KeyRatios {
            gross_margin_pct: py_round2(gross_margin),
            ebitda_margin_pct: py_round2(if total_revenue == 0.0 {
                0.0
            } else {
                ebitda / total_revenue * 100.0
            }),
            current_ratio: py_round2(current_ratio),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(items: &[(&str, f64)]) -> Items {
        items.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn revenue() -> Items {
        pairs(&[("sales", 500_000.0), ("service", 50_000.0)])
    }

    fn costs() -> Items {
        pairs(&[
            ("cogs", 300_000.0),
            ("opex", 100_000.0),
            ("admin", 20_000.0),
        ])
    }

    fn balance() -> Items {
        pairs(&[
            ("current_assets", 200_000.0),
            ("non_current_assets", 500_000.0),
            ("current_liabilities", 100_000.0),
            ("non_current_liabilities", 200_000.0),
        ])
    }

    #[test]
    fn blocks_are_built_from_the_item_maps() {
        let r = prepare_management_accounts("c1", "2026-Q1", &revenue(), &costs(), &balance());
        assert_eq!(r.pnl_statement.revenue, 550_000.0);
        assert_eq!(r.pnl_statement.cogs, 300_000.0);
        assert_eq!(r.pnl_statement.gross_profit, 250_000.0);
        assert_eq!(r.pnl_statement.opex, 100_000.0);
        assert_eq!(r.pnl_statement.ebitda, 130_000.0);
        assert_eq!(r.pnl_statement.gross_margin, 45.45);
        assert_eq!(r.balance_sheet.total_assets, 700_000.0);
        assert_eq!(r.balance_sheet.total_liabilities, 300_000.0);
        assert_eq!(r.balance_sheet.net_assets, 400_000.0);
        assert_eq!(r.working_capital.working_capital, 100_000.0);
        assert_eq!(r.working_capital.current_ratio, 2.0);
        assert_eq!(r.key_ratios.ebitda_margin_pct, 23.64);
    }

    #[test]
    fn missing_current_liabilities_defaults_to_one() {
        let balance = pairs(&[("current_assets", 200_000.0)]);
        let r = prepare_management_accounts("c1", "2026", &revenue(), &costs(), &balance);
        assert_eq!(r.working_capital.working_capital, 199_999.0);
        assert_eq!(r.working_capital.current_ratio, 200_000.0);
        assert_eq!(r.balance_sheet.total_liabilities, 0.0);
    }

    #[test]
    fn zero_current_liabilities_guards_the_ratio() {
        let balance = pairs(&[("current_assets", 50_000.0), ("current_liabilities", 0.0)]);
        let r = prepare_management_accounts("c1", "2026", &revenue(), &costs(), &balance);
        assert_eq!(r.working_capital.current_ratio, 0.0);
        assert_eq!(r.working_capital.working_capital, 50_000.0);
    }

    #[test]
    fn zero_revenue_guards_margins() {
        let r = prepare_management_accounts("c1", "2026", &Items::new(), &costs(), &balance());
        assert_eq!(r.pnl_statement.revenue, 0.0);
        assert_eq!(r.pnl_statement.gross_margin, 0.0);
        assert_eq!(r.key_ratios.ebitda_margin_pct, 0.0);
        assert_eq!(r.pnl_statement.ebitda, -420_000.0);
    }
}
