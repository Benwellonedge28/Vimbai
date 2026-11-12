//! trading-account-service `/generate`: net sales, cost of goods sold
//! and gross profit with the full cost build-up in `details`.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TradingAccountInput {
    pub company_id: String,
    pub period: String,
    pub sales: f64,
    pub sales_returns: f64,
    pub opening_stock: f64,
    pub purchases: f64,
    pub carriage_inward: f64,
    pub direct_wages: f64,
    pub factory_overhead: f64,
    pub closing_stock: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TradingAccountCosts {
    pub opening_stock: f64,
    pub purchases: f64,
    pub carriage_inward: f64,
    pub direct_wages: f64,
    pub factory_overhead: f64,
    pub closing_stock: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TradingAccountResult {
    pub company_id: String,
    pub period: String,
    pub net_sales: f64,
    pub cost_of_goods_sold: f64,
    pub gross_profit: f64,
    pub gross_profit_pct: f64,
    pub details: TradingAccountCosts,
}

pub fn generate_trading_account(input: &TradingAccountInput) -> TradingAccountResult {
    let net_sales = input.sales - input.sales_returns;
    let cost_of_goods_sold = input.opening_stock
        + input.purchases
        + input.carriage_inward
        + input.direct_wages
        + input.factory_overhead
        - input.closing_stock;
    let gross_profit = net_sales - cost_of_goods_sold;
    let gross_profit_pct = if net_sales == 0.0 {
        0.0
    } else {
        gross_profit / net_sales * 100.0
    };
    TradingAccountResult {
        company_id: input.company_id.clone(),
        period: input.period.clone(),
        net_sales,
        cost_of_goods_sold,
        gross_profit,
        gross_profit_pct: py_round2(gross_profit_pct),
        details: TradingAccountCosts {
            opening_stock: input.opening_stock,
            purchases: input.purchases,
            carriage_inward: input.carriage_inward,
            direct_wages: input.direct_wages,
            factory_overhead: input.factory_overhead,
            closing_stock: input.closing_stock,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> TradingAccountInput {
        TradingAccountInput {
            company_id: "c1".into(),
            period: "2026".into(),
            sales: 500_000.0,
            sales_returns: 20_000.0,
            opening_stock: 60_000.0,
            purchases: 250_000.0,
            carriage_inward: 10_000.0,
            direct_wages: 40_000.0,
            factory_overhead: 25_000.0,
            closing_stock: 45_000.0,
        }
    }

    #[test]
    fn cost_build_up_and_gross_profit() {
        let r = generate_trading_account(&input());
        assert_eq!(r.net_sales, 480_000.0);
        assert_eq!(r.cost_of_goods_sold, 340_000.0);
        assert_eq!(r.gross_profit, 140_000.0);
        assert_eq!(r.gross_profit_pct, 29.17);
        assert_eq!(r.details.closing_stock, 45_000.0);
    }

    #[test]
    fn zero_net_sales_guards_the_percentage() {
        let r = generate_trading_account(&TradingAccountInput {
            sales: 10_000.0,
            sales_returns: 10_000.0,
            ..input()
        });
        assert_eq!(r.net_sales, 0.0);
        assert_eq!(r.gross_profit_pct, 0.0);
    }

    #[test]
    fn negative_gross_profit_is_preserved() {
        let r = generate_trading_account(&TradingAccountInput {
            purchases: 500_000.0,
            ..input()
        });
        assert!(r.gross_profit < 0.0);
        assert!(r.gross_profit_pct < 0.0);
    }
}
