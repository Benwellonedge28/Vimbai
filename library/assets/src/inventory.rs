//! inventory-valuation-service: FIFO, LIFO and weighted average COGS.
use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Purchase {
    pub date: String,
    pub quantity: f64,
    pub unit_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sale {
    #[serde(default)]
    pub quantity: f64,
    #[serde(default)]
    pub unit_price: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValuationMethod {
    Fifo,
    Lifo,
    WeightedAverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryOutput {
    pub total_goods_available: f64,
    pub total_units_available: f64,
    pub total_units_sold: f64,
    pub cost_of_goods_sold: f64,
    pub ending_inventory: f64,
    pub ending_qty: f64,
    pub total_sales_revenue: f64,
    pub gross_profit: f64,
    pub gross_margin: f64,
}

/// Pure port of the service. Purchases are consumed oldest-first (FIFO)
/// or newest-first (LIFO) after opening stock, exactly as the Python does.
pub fn inventory_valuation(
    method: ValuationMethod,
    opening_qty: f64,
    opening_inventory: f64,
    purchases: &[Purchase],
    sales: &[Sale],
) -> InventoryOutput {
    let mut layers: Vec<&Purchase> = purchases.iter().collect();
    layers.sort_by(|a, b| a.date.cmp(&b.date));
    let total_units = opening_qty + layers.iter().map(|p| p.quantity).sum::<f64>();
    let total_cost =
        opening_inventory + layers.iter().map(|p| p.quantity * p.unit_cost).sum::<f64>();
    let units_sold: f64 = sales.iter().map(|s| s.quantity).sum();
    let sales_revenue: f64 = sales.iter().map(|s| s.quantity * s.unit_price).sum();

    let cogs = match method {
        ValuationMethod::Fifo => {
            let mut remaining = units_sold;
            let mut cogs = 0.0;
            let remaining_cost = opening_inventory;
            let remaining_qty = opening_qty;
            if remaining > 0.0 && remaining_qty > 0.0 {
                let taken = remaining.min(remaining_qty);
                cogs += remaining_cost * (taken / remaining_qty);
                remaining -= taken;
            }
            for p in &layers {
                if remaining <= 0.0 {
                    break;
                }
                let taken = remaining.min(p.quantity);
                cogs += taken * p.unit_cost;
                remaining -= taken;
            }
            cogs
        }
        ValuationMethod::Lifo => {
            let mut remaining = units_sold;
            let mut cogs = 0.0;
            for p in layers.iter().rev() {
                if remaining <= 0.0 {
                    break;
                }
                let taken = remaining.min(p.quantity);
                cogs += taken * p.unit_cost;
                remaining -= taken;
            }
            if remaining > 0.0 && opening_qty > 0.0 {
                let taken = remaining.min(opening_qty);
                cogs += taken * (opening_inventory / opening_qty);
            }
            cogs
        }
        ValuationMethod::WeightedAverage => {
            let avg = if total_units != 0.0 {
                total_cost / total_units
            } else {
                0.0
            };
            avg * units_sold
        }
    };

    let ending_qty = total_units - units_sold;
    let ending_inv = match method {
        ValuationMethod::WeightedAverage => {
            let avg = if total_units != 0.0 {
                total_cost / total_units
            } else {
                0.0
            };
            avg * ending_qty
        }
        _ => total_cost - cogs,
    };
    let gross_profit = sales_revenue - cogs;
    let gross_margin = if sales_revenue != 0.0 {
        gross_profit / sales_revenue * 100.0
    } else {
        0.0
    };

    InventoryOutput {
        total_goods_available: py_round2(total_cost),
        total_units_available: total_units,
        total_units_sold: units_sold,
        cost_of_goods_sold: py_round2(cogs),
        ending_inventory: py_round2(ending_inv),
        ending_qty,
        total_sales_revenue: py_round2(sales_revenue),
        gross_profit: py_round2(gross_profit),
        gross_margin: py_round2(gross_margin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> (Vec<Purchase>, Vec<Sale>) {
        (
            vec![
                Purchase {
                    date: "2026-01-01".into(),
                    quantity: 100.0,
                    unit_cost: 10.0,
                },
                Purchase {
                    date: "2026-02-01".into(),
                    quantity: 100.0,
                    unit_cost: 12.0,
                },
            ],
            vec![Sale {
                quantity: 150.0,
                unit_price: 20.0,
            }],
        )
    }

    #[test]
    fn fifo_consumes_opening_then_oldest() {
        let (p, s) = data();
        let r = inventory_valuation(ValuationMethod::Fifo, 50.0, 400.0, &p, &s);
        // opening 50 @ 8 -> 400, then 100 @ 10 = 1000 -> COGS 1400
        assert_eq!(r.cost_of_goods_sold, 1400.0);
        // total 2600 - 1400 = 1200 (the 100 @ 12 layer)
        assert_eq!(r.ending_inventory, 1200.0);
        assert_eq!(r.ending_qty, 100.0);
    }

    #[test]
    fn lifo_consumes_newest_first() {
        let (p, s) = data();
        let r = inventory_valuation(ValuationMethod::Lifo, 50.0, 400.0, &p, &s);
        // 100 @ 12 = 1200, then 50 @ 10 = 500 -> 1700
        assert_eq!(r.cost_of_goods_sold, 1700.0);
    }

    #[test]
    fn weighted_average_cost() {
        let (p, s) = data();
        let r = inventory_valuation(ValuationMethod::WeightedAverage, 50.0, 400.0, &p, &s);
        // total 2600 / 250 units = 10.4; COGS 150 * 10.4 = 1560
        assert_eq!(r.cost_of_goods_sold, 1560.0);
        assert_eq!(r.ending_inventory, 1040.0);
        assert_eq!(r.gross_margin, 48.0);
    }
}
