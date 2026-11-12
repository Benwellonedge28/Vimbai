//! Inventory valuation: FIFO, LIFO and weighted average.

use serde::{Deserialize, Serialize};

use vimbai_common::Money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PurchaseRecord {
    /// ISO date; purchases are consumed in date order.
    pub date: String,
    pub quantity: i64,
    pub unit_cost: Money,
}

impl PurchaseRecord {
    pub fn new(date: &str, quantity: i64, unit_cost: Money) -> PurchaseRecord {
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
    pub unit_price: Money,
}

impl SaleRecord {
    pub fn new(date: &str, quantity: i64, unit_price: Money) -> SaleRecord {
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
    pub opening_inventory: Money,
    pub opening_qty: i64,
}

impl ValuationRequest {
    pub fn new(company_id: &str, method: ValuationMethod) -> ValuationRequest {
        ValuationRequest {
            company_id: company_id.to_string(),
            method,
            purchases: Vec::new(),
            sales: Vec::new(),
            opening_inventory: Money::zero(),
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

    pub fn with_opening(mut self, inventory: Money, quantity: i64) -> ValuationRequest {
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
    let zero = Money::zero();
    let mut purchases = request.purchases.clone();
    purchases.sort_by(|a, b| a.date.cmp(&b.date));

    let total_units = request.opening_qty + purchases.iter().map(|p| p.quantity).sum::<i64>();
    let layer_costs: Vec<Money> = purchases
        .iter()
        .map(|p| p.unit_cost.times(&Money::from_i64(p.quantity)))
        .collect();
    let total_cost = request.opening_inventory.plus(&Money::sum(&layer_costs));
    let units_sold: i64 = request.sales.iter().map(|s| s.quantity).sum();
    let sale_values: Vec<Money> = request
        .sales
        .iter()
        .map(|s| s.unit_price.times(&Money::from_i64(s.quantity)))
        .collect();
    let sales_revenue = Money::sum(&sale_values);

    let cogs = match request.method {
        ValuationMethod::Fifo => {
            let mut remaining_units = units_sold;
            let mut cogs = zero.clone();
            if remaining_units > 0 && request.opening_qty > 0 {
                let taken = remaining_units.min(request.opening_qty);
                // the opening layer is split in proportion to the units taken
                cogs = cogs.plus(
                    &request
                        .opening_inventory
                        .times(&Money::from_i64(taken))
                        .over(&Money::from_i64(request.opening_qty)),
                );
                remaining_units -= taken;
            }
            for purchase in &purchases {
                if remaining_units <= 0 {
                    break;
                }
                let taken = remaining_units.min(purchase.quantity);
                cogs = cogs.plus(&purchase.unit_cost.times(&Money::from_i64(taken)));
                remaining_units -= taken;
            }
            cogs
        }
        ValuationMethod::Lifo => {
            let mut remaining_units = units_sold;
            let mut cogs = zero.clone();
            for purchase in purchases.iter().rev() {
                if remaining_units <= 0 {
                    break;
                }
                let taken = remaining_units.min(purchase.quantity);
                cogs = cogs.plus(&purchase.unit_cost.times(&Money::from_i64(taken)));
                remaining_units -= taken;
            }
            if remaining_units > 0 && request.opening_qty > 0 {
                let taken = remaining_units.min(request.opening_qty);
                cogs = cogs.plus(
                    &request
                        .opening_inventory
                        .over(&Money::from_i64(request.opening_qty))
                        .times(&Money::from_i64(taken)),
                );
            }
            cogs
        }
        ValuationMethod::WeightedAverage => {
            let average = if total_units != 0 {
                total_cost.over(&Money::from_i64(total_units))
            } else {
                zero.clone()
            };
            average.times(&Money::from_i64(units_sold))
        }
    };

    let ending_qty = total_units - units_sold;
    let ending_inventory = match request.method {
        ValuationMethod::WeightedAverage => {
            let average = if total_units != 0 {
                total_cost.over(&Money::from_i64(total_units))
            } else {
                zero.clone()
            };
            average.times(&Money::from_i64(ending_qty))
        }
        _ => total_cost.minus(&cogs),
    };

    let gross_profit = sales_revenue.minus(&cogs);
    let gross_margin = gross_profit.ratio_pct(&sales_revenue, 2);

    ValuationResult {
        company_id: request.company_id.clone(),
        method: request.method.value().to_string(),
        total_goods_available: total_cost.round(2).to_f64_lossy(),
        total_units_available: total_units,
        total_units_sold: units_sold,
        cost_of_goods_sold: cogs.round(2).to_f64_lossy(),
        ending_inventory: ending_inventory.round(2).to_f64_lossy(),
        ending_qty,
        total_sales_revenue: sales_revenue.round(2).to_f64_lossy(),
        gross_profit: gross_profit.round(2).to_f64_lossy(),
        gross_margin: gross_margin.round(2).to_f64_lossy(),
    }
}

/// The service's `/calculate` body.
pub fn calculate_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut request = ValuationRequest::new(
        payload
            .get("company_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
        ValuationMethod::parse(
            payload
                .get("method")
                .and_then(|v| v.as_str())
                .unwrap_or("weighted_average"),
        ),
    );
    let money = |value: &serde_json::Value| {
        Money::parse(&value.to_string()).unwrap_or_else(|_| Money::zero())
    };
    if let Some(list) = payload.get("purchases").and_then(|v| v.as_array()) {
        request.purchases = list
            .iter()
            .map(|p| PurchaseRecord {
                date: p
                    .get("date")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                quantity: p.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0),
                unit_cost: p.get("unit_cost").map(money).unwrap_or_else(Money::zero),
            })
            .collect();
    }
    if let Some(list) = payload.get("sales").and_then(|v| v.as_array()) {
        request.sales = list
            .iter()
            .map(|s| SaleRecord {
                date: s
                    .get("date")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                quantity: s.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0),
                unit_price: s.get("unit_price").map(money).unwrap_or_else(Money::zero),
            })
            .collect();
    }
    request.opening_inventory = payload
        .get("opening_inventory")
        .map(money)
        .unwrap_or_else(Money::zero);
    request.opening_qty = payload
        .get("opening_qty")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    serde_json::to_value(calculate(&request)).map_err(|e| e.to_string())
}
