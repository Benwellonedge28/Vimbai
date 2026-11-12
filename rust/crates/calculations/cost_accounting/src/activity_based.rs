#![forbid(unsafe_code)]
//! Activity-based costing, ported from activity-based-costing-service
//! `/calculate` (three-level driver hierarchy).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbcActivity {
    pub activity_id: String,
    pub activity_name: String,
    pub activity_type: String,
    pub cost_pool: f64,
    pub cost_driver: String,
    pub driver_volume: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbcProduct {
    pub product_id: String,
    pub product_name: String,
    pub unit_level_drivers: BTreeMap<String, i64>,
    pub batch_level_drivers: BTreeMap<String, i64>,
    pub product_level_activities: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbcRequest {
    pub company_id: String,
    pub activities: Vec<AbcActivity>,
    pub products: Vec<AbcProduct>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityRate {
    pub activity_id: String,
    pub activity_name: String,
    pub activity_type: String,
    pub rate_per_driver: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductCost {
    pub product_id: String,
    pub product_name: String,
    pub unit_level_cost: f64,
    pub batch_level_cost: f64,
    pub product_level_cost: f64,
    pub total_product_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbcResponse {
    pub company_id: String,
    pub activity_rates: Vec<ActivityRate>,
    pub product_costs: Vec<ProductCost>,
    pub unit_costs: BTreeMap<String, f64>,
    pub total_overhead_allocated: f64,
}

/// Rate per driver unit (`cost_pool / driver_volume`, 0 when volume 0).
fn rate(activity: &AbcActivity) -> f64 {
    if activity.driver_volume != 0 {
        activity.cost_pool / activity.driver_volume as f64
    } else {
        0.0
    }
}

/// ABC calculation (activity-based-costing-service `/calculate`).
/// `unit_costs` sums each product's raw unit-level driver volumes
/// (NOT the priced cost) — quirk kept for parity.
pub fn calculate_abc(request: &AbcRequest) -> AbcResponse {
    let activity_rates = request
        .activities
        .iter()
        .map(|a| ActivityRate {
            activity_id: a.activity_id.clone(),
            activity_name: a.activity_name.clone(),
            activity_type: a.activity_type.clone(),
            rate_per_driver: py_round2(rate(a)),
        })
        .collect();

    let mut product_costs = Vec::with_capacity(request.products.len());
    let mut unit_costs = BTreeMap::new();
    let mut total_overhead = 0.0;
    for p in &request.products {
        let mut unit_cost = 0.0;
        let mut batch_cost = 0.0;
        for a in &request.activities {
            let r = rate(a);
            match a.activity_type.as_str() {
                "unit_level" => {
                    unit_cost += p
                        .unit_level_drivers
                        .get(&a.activity_id)
                        .copied()
                        .unwrap_or(0) as f64
                        * r;
                }
                "batch_level" => {
                    batch_cost += p
                        .batch_level_drivers
                        .get(&a.activity_id)
                        .copied()
                        .unwrap_or(0) as f64
                        * r;
                }
                _ => {}
            }
        }
        let product_level_cost: f64 = p.product_level_activities.values().sum();
        let product_cost = unit_cost + batch_cost + product_level_cost;
        total_overhead += product_cost;
        product_costs.push(ProductCost {
            product_id: p.product_id.clone(),
            product_name: p.product_name.clone(),
            unit_level_cost: py_round2(unit_cost),
            batch_level_cost: py_round2(batch_cost),
            product_level_cost: py_round2(product_level_cost),
            total_product_cost: py_round2(product_cost),
        });
        let raw_units: f64 = p.unit_level_drivers.values().map(|v| *v as f64).sum();
        unit_costs.insert(p.product_id.clone(), py_round2(raw_units));
    }

    AbcResponse {
        company_id: request.company_id.clone(),
        activity_rates,
        product_costs,
        unit_costs,
        total_overhead_allocated: total_overhead,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> AbcRequest {
        AbcRequest {
            company_id: "c1".into(),
            activities: vec![
                AbcActivity {
                    activity_id: " machining".into(), // deliberately unsorted key
                    activity_name: "Machining".into(),
                    activity_type: "unit_level".into(),
                    cost_pool: 10000.0,
                    cost_driver: "machine_hours".into(),
                    driver_volume: 1000,
                },
                AbcActivity {
                    activity_id: "setup".into(),
                    activity_name: "Setup".into(),
                    activity_type: "batch_level".into(),
                    cost_pool: 5000.0,
                    cost_driver: "batches".into(),
                    driver_volume: 50,
                },
                AbcActivity {
                    activity_id: "design".into(),
                    activity_name: "Design".into(),
                    activity_type: "product_level".into(),
                    cost_pool: 2000.0,
                    cost_driver: "designs".into(),
                    driver_volume: 0,
                },
            ],
            products: vec![AbcProduct {
                product_id: "p1".into(),
                product_name: "Widget".into(),
                unit_level_drivers: [(" machining".to_string(), 100)].into_iter().collect(),
                batch_level_drivers: [("setup".to_string(), 5)].into_iter().collect(),
                product_level_activities: [("design fee".to_string(), 500.0)].into_iter().collect(),
            }],
        }
    }

    #[test]
    fn abc_rates_and_costs() {
        let r = calculate_abc(&req());
        assert_eq!(r.activity_rates[0].rate_per_driver, 10.0);
        assert_eq!(r.activity_rates[1].rate_per_driver, 100.0);
        assert_eq!(r.activity_rates[2].rate_per_driver, 0.0); // zero volume guard
        let p = &r.product_costs[0];
        // unit: 100 hrs * 10 = 1000; batch: 5 * 100 = 500; product: 500
        assert_eq!(p.unit_level_cost, 1000.0);
        assert_eq!(p.batch_level_cost, 500.0);
        assert_eq!(p.product_level_cost, 500.0);
        assert_eq!(p.total_product_cost, 2000.0);
        assert_eq!(r.total_overhead_allocated, 2000.0);
    }

    #[test]
    fn unit_costs_sums_raw_driver_volumes() {
        // Parity quirk: unit_costs[p] = sum of unit driver VOLUMES, not cost
        let r = calculate_abc(&req());
        assert_eq!(r.unit_costs["p1"], 100.0);
    }
}
