#![forbid(unsafe_code)]
//! Throughput accounting (Theory of Constraints), ported from
//! throughput-accounting-service including the greedy optimal-mix
//! allocation over the constraint.

use serde::{Deserialize, Serialize};
use vimbai_common::{py_round, py_round2};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub name: String,
    pub selling_price: f64,
    pub material_cost: f64,
    pub time_on_constraint: f64,
    pub demand: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputRequest {
    pub company_id: String,
    pub operating_expenses: f64,
    pub products: Vec<Product>,
    #[serde(default = "default_minutes")]
    pub available_constraint_minutes: f64,
}

fn default_minutes() -> f64 {
    480.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedProduct {
    pub name: String,
    pub throughput_per_unit: f64,
    pub throughput_per_minute: f64,
    pub demand: i64,
    pub time_on_constraint: f64,
    pub total_throughput_possible: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixEntry {
    pub product: String,
    pub produce: i64,
    pub demand: i64,
    pub throughput_contribution: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingEntry {
    pub name: String,
    pub tpm: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputResult {
    pub company_id: String,
    pub total_throughput: f64,
    pub operating_expenses: f64,
    pub net_profit: f64,
    pub roi: f64,
    pub product_ranking: Vec<RankingEntry>,
    pub optimal_mix: Vec<MixEntry>,
    pub constraint_utilization: f64,
}

/// Throughput analysis with greedy constraint allocation
/// (throughput-accounting-service `/analyze`). Ranking uses a stable
/// sort by throughput-per-minute descending, matching Python.
pub fn analyze_throughput(req: &ThroughputRequest) -> ThroughputResult {
    let mut products: Vec<RankedProduct> = req
        .products
        .iter()
        .map(|p| {
            let tpu = p.selling_price - p.material_cost;
            let tpm = if p.time_on_constraint > 0.0 {
                tpu / p.time_on_constraint
            } else {
                0.0
            };
            RankedProduct {
                name: p.name.clone(),
                throughput_per_unit: py_round2(tpu),
                throughput_per_minute: py_round2(tpm),
                demand: p.demand,
                time_on_constraint: p.time_on_constraint,
                total_throughput_possible: py_round2(tpu * p.demand as f64),
            }
        })
        .collect();

    products.sort_by(|a, b| {
        b.throughput_per_minute
            .partial_cmp(&a.throughput_per_minute)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut remaining_minutes = req.available_constraint_minutes;
    let mut optimal_mix = Vec::with_capacity(products.len());
    let mut total_throughput = 0.0;
    for p in &products {
        let minutes_needed = p.demand as f64 * p.time_on_constraint;
        let produce = if remaining_minutes >= minutes_needed {
            remaining_minutes -= minutes_needed;
            p.demand
        } else if p.time_on_constraint > 0.0 {
            let produced = (remaining_minutes / p.time_on_constraint) as i64;
            remaining_minutes = 0.0;
            produced
        } else {
            0
        };
        let contribution = p.throughput_per_unit * produce as f64;
        total_throughput += contribution;
        optimal_mix.push(MixEntry {
            product: p.name.clone(),
            produce,
            demand: p.demand,
            throughput_contribution: py_round2(contribution),
        });
    }

    let net_profit = total_throughput - req.operating_expenses;
    let roi = if req.operating_expenses != 0.0 {
        net_profit / req.operating_expenses * 100.0
    } else {
        0.0
    };
    let utilization = if req.available_constraint_minutes != 0.0 {
        (req.available_constraint_minutes - remaining_minutes) / req.available_constraint_minutes
            * 100.0
    } else {
        0.0
    };

    ThroughputResult {
        company_id: req.company_id.clone(),
        total_throughput: py_round2(total_throughput),
        operating_expenses: py_round2(req.operating_expenses),
        net_profit: py_round2(net_profit),
        roi: py_round2(roi),
        product_ranking: products
            .iter()
            .map(|p| RankingEntry {
                name: p.name.clone(),
                tpm: p.throughput_per_minute,
            })
            .collect(),
        optimal_mix,
        constraint_utilization: py_round(utilization, 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> ThroughputRequest {
        ThroughputRequest {
            company_id: "c1".into(),
            operating_expenses: 500.0,
            available_constraint_minutes: 480.0,
            products: vec![
                Product {
                    name: "A".into(),
                    selling_price: 100.0,
                    material_cost: 40.0,
                    time_on_constraint: 2.0,
                    demand: 100,
                },
                Product {
                    name: "B".into(),
                    selling_price: 80.0,
                    material_cost: 30.0,
                    time_on_constraint: 1.0,
                    demand: 200,
                },
            ],
        }
    }

    #[test]
    fn ranking_by_tpm() {
        let r = analyze_throughput(&req());
        // A: tpu 60, tpm 30; B: tpu 50, tpm 50 -> B first
        assert_eq!(r.product_ranking[0].name, "B");
        assert_eq!(r.product_ranking[0].tpm, 50.0);
        assert_eq!(r.product_ranking[1].name, "A");
    }

    #[test]
    fn greedy_allocation() {
        let r = analyze_throughput(&req());
        // B first (tpm 50): 200 min, fully produced, 280 remain.
        // A (tpm 30): needs 200 min <= 280, fully produced, 80 remain.
        assert_eq!(r.optimal_mix[0].product, "B");
        assert_eq!(r.optimal_mix[0].produce, 200);
        assert_eq!(r.optimal_mix[0].throughput_contribution, 10000.0);
        assert_eq!(r.optimal_mix[1].product, "A");
        assert_eq!(r.optimal_mix[1].produce, 100);
        assert_eq!(r.optimal_mix[1].throughput_contribution, 6000.0);
        assert_eq!(r.total_throughput, 16000.0);
        assert_eq!(r.net_profit, 15500.0);
        assert_eq!(r.roi, 3100.0);
        assert_eq!(r.constraint_utilization, 83.3);
    }

    #[test]
    fn idle_constraint_utilization() {
        let mut rq = req();
        rq.products = vec![Product {
            name: "B".into(),
            selling_price: 80.0,
            material_cost: 30.0,
            time_on_constraint: 1.0,
            demand: 100,
        }];
        let r = analyze_throughput(&rq);
        // 100 minutes used of 480 -> 20.8%
        assert_eq!(r.constraint_utilization, 20.8);
        assert_eq!(r.optimal_mix[0].produce, 100);
    }

    #[test]
    fn zero_time_guard() {
        let mut rq = req();
        rq.available_constraint_minutes = 0.0;
        rq.products = vec![Product {
            name: "X".into(),
            selling_price: 10.0,
            material_cost: 5.0,
            time_on_constraint: 0.0,
            demand: 10,
        }];
        let r = analyze_throughput(&rq);
        // Python parity: minutes_needed == 0, and 0 >= 0, so the full
        // demand is "produced" and throughput accrues.
        assert_eq!(r.optimal_mix[0].produce, 10);
        assert_eq!(r.total_throughput, 50.0);
        assert_eq!(r.net_profit, -450.0);
        assert_eq!(r.roi, -90.0);
        assert_eq!(r.constraint_utilization, 0.0);
    }
}
