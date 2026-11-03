//! Inventory valuation: FIFO, LIFO and weighted average.

use serde::{Deserialize, Serialize};

use crate::money::money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PurchaseRecord {
    /// ISO date; purchases are consumed in date order.
    pub date: String,
    pub quantity: i64,
    pub unit_cost: f64,
}

impl PurchaseRecord {
    pub fn new(date: &str, quantity: i64, unit_cost: f64) -> PurchaseRecord {
        PurchaseRecord {
            date: date.to_string(),
            quantity,
            unit_cost,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaleRecord {
    pub date: String,
    pub quantity: i64,
    pub unit_price: f64,
}

impl SaleRecord {
    pub fn new(date: &str, quantity: i64, unit_price: f64) -> SaleRecord {
        SaleRecord {
            date: date.to_string(),
            quantity,
            unit_price,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValuationMethod {
    Fifo,
    Lifo,
    WeightedAverage,
}

impl ValuationMethod {
    pub fn parse(text: &str) -> ValuationMethod {
        match text {
            "fifo" => ValuationMethod::Fifo,
            "lifo" => ValuationMethod::Lifo,
            _ => ValuationMethod::WeightedAverage,
        }
    }

    pub fn value(self) -> &'static str {
        match self {
            ValuationMethod::Fifo => "fifo",
            ValuationMethod::Lifo => "lifo",
            ValuationMethod::WeightedAverage => "weighted_average",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValuationRequest {
    pub company_id: String,
    pub method: ValuationMethod,
    pub purchases: Vec<PurchaseRecord>,
    pub sales: Vec<SaleRecord>,
    pub opening_inventory: f64,
    pub opening_qty: i64,
}

impl ValuationRequest {
    pub fn new(company_id: &str, method: ValuationMethod) -> ValuationRequest {
        ValuationRequest {
            company_id: company_id.to_string(),
            method,
            purchases: Vec::new(),
            sales: Vec::new(),
            opening_inventory: 0.0,
            opening_qty: 0,
        }
    }

    pub fn with_purchases(mut self, purchases: Vec<PurchaseRecord>) -> ValuationRequest {
        self.purchases = purchases;
        self
    }

    pub fn with_sales(mut self, sales: Vec<SaleRecord>) -> ValuationRequest {
        self.sales = sales;
        self
    }

    pub fn with_opening(mut self, inventory: f64, quantity: i64) -> ValuationRequest {
        self.opening_inventory = inventory;
        self.opening_qty = quantity;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValuationResult {
    pub company_id: String,
    pub method: String,
    pub total_goods_available: f64,
    pub total_units_available: i64,
    pub total_units_sold: i64,
    pub cost_of_goods_sold: f64,
    pub ending_inventory: f64,
    pub ending_qty: i64,
    pub total_sales_revenue: f64,
    pub gross_profit: f64,
    pub gross_margin: f64,
}

/// Value the inventory.
///
/// Preserved quirk: LIFO consumes purchases first and only then the opening
/// layer, while FIFO consumes opening stock first. LIFO therefore reports a
/// different cost of sales on identical data, and neither method checks that
/// there is enough stock to sell, so an oversell quietly produces a negative
/// ending quantity.
pub fn calculate(request: &ValuationRequest) -> ValuationResult {
    let mut purchases = request.purchases.clone();
    purchases.sort_by(|a, b| a.date.cmp(&b.date));

    let total_units = request.opening_qty + purchases.iter().map(|p| p.quantity).sum::<i64>();
    let total_cost = request.opening_inventory
        + purchases
            .iter()
            .map(|p| p.quantity as f64 * p.unit_cost)
            .sum::<f64>();
    let units_sold: i64 = request.sales.iter().map(|s| s.quantity).sum();
    let sales_revenue: f64 = request
        .sales
        .iter()
        .map(|s| s.quantity as f64 * s.unit_price)
        .sum();

    let cogs = match request.method {
        ValuationMethod::Fifo => {
            let mut remaining_units = units_sold;
            let mut cogs = 0.0;
            if remaining_units > 0 && request.opening_qty > 0 {
                let taken = remaining_units.min(request.opening_qty);
                cogs += request.opening_inventory * (taken as f64 / request.opening_qty as f64);
                remaining_units -= taken;
            }
            for purchase in &purchases {
                if remaining_units <= 0 {
                    break;
                }
                let taken = remaining_units.min(purchase.quantity);
                cogs += taken as f64 * purchase.unit_cost;
                remaining_units -= taken;
            }
            cogs
        }
        ValuationMethod::Lifo => {
            let mut remaining_units = units_sold;
            let mut cogs = 0.0;
            for purchase in purchases.iter().rev() {
                if remaining_units <= 0 {
                    break;
                }
                let taken = remaining_units.min(purchase.quantity);
                cogs += taken as f64 * purchase.unit_cost;
                remaining_units -= taken;
            }
            if remaining_units > 0 && request.opening_qty > 0 {
                let taken = remaining_units.min(request.opening_qty);
                cogs += taken as f64 * (request.opening_inventory / request.opening_qty as f64);
            }
            cogs
        }
        ValuationMethod::WeightedAverage => {
            let average = if total_units != 0 {
                total_cost / total_units as f64
            } else {
                0.0
            };
            average * units_sold as f64
        }
    };

    let ending_qty = total_units - units_sold;
    let ending_inventory = match request.method {
        ValuationMethod::WeightedAverage => {
            let average = if total_units != 0 {
                total_cost / total_units as f64
            } else {
                0.0
            };
            average * ending_qty as f64
        }
        _ => total_cost - cogs,
    };

    let gross_profit = sales_revenue - cogs;
    let gross_margin = if sales_revenue != 0.0 {
        gross_profit / sales_revenue * 100.0
    } else {
        0.0
    };

    ValuationResult {
        company_id: request.company_id.clone(),
        method: request.method.value().to_string(),
        total_goods_available: money(total_cost),
        total_units_available: total_units,
        total_units_sold: units_sold,
        cost_of_goods_sold: money(cogs),
        ending_inventory: money(ending_inventory),
        ending_qty,
        total_sales_revenue: money(sales_revenue),
        gross_profit: money(gross_profit),
        gross_margin: money(gross_margin),
    }
}
