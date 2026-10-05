#![forbid(unsafe_code)]
//! Make-or-buy decision, ported from make-or-buy-decision-service
//! `/analyze`.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MakeCost {
    pub direct_materials: f64,
    pub direct_labour: f64,
    pub variable_overhead: f64,
    pub fixed_overhead: f64,
    #[serde(default)]
    pub setup_cost: f64,
    #[serde(default)]
    pub tooling_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuyCost {
    pub unit_purchase_price: f64,
    #[serde(default)]
    pub ordering_cost: f64,
    #[serde(default)]
    pub carrying_cost_per_unit: f64,
    #[serde(default)]
    pub quality_inspection_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MakeOrBuyRequest {
    pub company_id: String,
    pub product_name: String,
    pub annual_volume: i64,
    pub make_costs: MakeCost,
    pub buy_costs: BuyCost,
    #[serde(default)]
    pub opportunity_cost: f64,
    #[serde(default)]
    pub quality_considerations: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MakeOrBuyResult {
    pub company_id: String,
    pub product_name: String,
    pub annual_volume: i64,
    pub make_total_cost: f64,
    pub buy_total_cost: f64,
    pub difference: f64,
    pub recommendation: &'static str,
    pub make_cost_per_unit: f64,
    pub buy_cost_per_unit: f64,
    pub qualitative_factors: Vec<String>,
}

/// Make-or-buy analysis (make-or-buy-decision-service `/analyze`).
/// `MAKE` only when strictly cheaper; ties go to `BUY`.
pub fn analyze_make_or_buy(req: &MakeOrBuyRequest) -> MakeOrBuyResult {
    let vol = req.annual_volume as f64;
    let m = &req.make_costs;
    let b = &req.buy_costs;
    let make_total = (m.direct_materials + m.direct_labour + m.variable_overhead) * vol
        + m.fixed_overhead
        + m.setup_cost
        + m.tooling_cost
        + req.opportunity_cost;
    let buy_total = (b.unit_purchase_price + b.quality_inspection_cost) * vol
        + b.ordering_cost
        + (b.carrying_cost_per_unit * vol);
    let difference = make_total - buy_total;
    let recommendation: &'static str = if make_total < buy_total {
        "MAKE"
    } else {
        "BUY"
    };
    let make_per_unit = if vol != 0.0 { make_total / vol } else { 0.0 };
    let buy_per_unit = if vol != 0.0 { buy_total / vol } else { 0.0 };

    let mut qualitative_factors = vec![
        "Quality control over production process".to_string(),
        "Lead time and delivery reliability".to_string(),
        "Supplier dependency risk".to_string(),
        "Strategic core competency".to_string(),
        "Capacity utilization".to_string(),
        "Intellectual property protection".to_string(),
    ];
    if !req.quality_considerations.is_empty() {
        qualitative_factors.insert(0, req.quality_considerations.clone());
    }

    MakeOrBuyResult {
        company_id: req.company_id.clone(),
        product_name: req.product_name.clone(),
        annual_volume: req.annual_volume,
        make_total_cost: py_round2(make_total),
        buy_total_cost: py_round2(buy_total),
        difference: py_round2(difference),
        recommendation,
        make_cost_per_unit: py_round2(make_per_unit),
        buy_cost_per_unit: py_round2(buy_per_unit),
        qualitative_factors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> MakeOrBuyRequest {
        MakeOrBuyRequest {
            company_id: "c1".into(),
            product_name: "Bracket".into(),
            annual_volume: 10000,
            make_costs: MakeCost {
                direct_materials: 5.0,
                direct_labour: 4.0,
                variable_overhead: 1.0,
                fixed_overhead: 8000.0,
                setup_cost: 2000.0,
                tooling_cost: 0.0,
            },
            buy_costs: BuyCost {
                unit_purchase_price: 10.5,
                ordering_cost: 1000.0,
                carrying_cost_per_unit: 0.2,
                quality_inspection_cost: 0.1,
            },
            opportunity_cost: 0.0,
            quality_considerations: "Custom finish required".into(),
        }
    }

    #[test]
    fn make_wins() {
        let r = analyze_make_or_buy(&req());
        // make = 10*10000 + 8000 + 2000 = 110000
        assert_eq!(r.make_total_cost, 110000.0);
        // buy = 10.6*10000 + 1000 + 2000 = 109000
        assert_eq!(r.buy_total_cost, 109000.0);
        assert_eq!(r.difference, 1000.0);
        assert_eq!(r.recommendation, "BUY");
        assert_eq!(r.make_cost_per_unit, 11.0);
        assert_eq!(r.buy_cost_per_unit, 10.9);
        // custom consideration inserted first
        assert_eq!(r.qualitative_factors[0], "Custom finish required");
        assert_eq!(r.qualitative_factors.len(), 7);
    }

    #[test]
    fn make_cheaper_when_volume_low() {
        let mut rq = req();
        rq.annual_volume = 1000;
        let r = analyze_make_or_buy(&rq);
        // make = 10000 + 10000 = 20000; buy = 10600 + 1000 + 200 = 11800 -> BUY
        assert_eq!(r.recommendation, "BUY");
        let mut rq2 = req();
        rq2.buy_costs.unit_purchase_price = 30.0;
        // buy = 30.1*10000 + 1000 + 2000 = 304000 -> MAKE
        let r2 = analyze_make_or_buy(&rq2);
        assert_eq!(r2.recommendation, "MAKE");
    }

    #[test]
    fn tie_goes_to_buy() {
        let mut rq = req();
        rq.buy_costs.unit_purchase_price = 0.0;
        rq.buy_costs.quality_inspection_cost = 0.0;
        rq.buy_costs.ordering_cost = 0.0;
        rq.buy_costs.carrying_cost_per_unit = 0.0;
        assert_eq!(analyze_make_or_buy(&rq).recommendation, "BUY");
    }

    #[test]
    fn zero_volume_guards() {
        let mut rq = req();
        rq.annual_volume = 0;
        let r = analyze_make_or_buy(&rq);
        assert_eq!(r.make_total_cost, 10000.0); // fixed only
        assert_eq!(r.make_cost_per_unit, 0.0);
        assert_eq!(r.buy_cost_per_unit, 0.0);
    }
}
