#![forbid(unsafe_code)]
//! Merger valuation, ported from merger-valuation-service.
//!
//! Kept as shipped:
//!
//! * The three valuation methods are fixed scalings of target equity - 1.1x for
//!   "dcf", `1 + deal_premium` for "comparables", 0.9x for "asset" - with fixed
//!   0.4/0.4/0.2 weights. They are not computed from the financials given.
//! * IRR projections are one ratio (`comps / offer - 1`) scaled by 100, 50 and
//!   33.33, so the "3 year IRR" is a third of the one year figure, not a
//!   compounded return.
//! * The accretive/dilutive test compares equity-per-revenue-dollar of the
//!   combined entity with the acquirer's own; it is an EPS proxy, not EPS.
//!
//! The service also computed `acquirer_new_pe` and never returned it; the dead
//! calculation is dropped here instead of being carried along.
//!
//! Every division that the Python left unguarded (target EBITDA, acquirer
//! revenue, target EBITDA again inside the synergy value) yields 0.0 on a zero
//! denominator rather than crashing the endpoint.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round;

fn r2(value: f64) -> f64 {
    py_round(value, 2)
}

fn r4(value: f64) -> f64 {
    py_round(value, 4)
}

fn safe_div(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergerRequest {
    pub acquirer_id: String,
    pub target_id: String,
    pub acquirer_equity: f64,
    pub target_equity: f64,
    pub acquirer_ebitda: f64,
    pub target_ebitda: f64,
    pub acquirer_revenue: f64,
    pub target_revenue: f64,
    pub synergies_ebitda: f64,
    pub acquirer_pe: f64,
    pub target_pe: f64,
    pub deal_premium: f64,
    pub transaction_costs: f64,
    pub integration_costs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValuationMethod {
    pub value: f64,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValuationMethods {
    pub dcf: ValuationMethod,
    pub comparables: ValuationMethod,
    pub asset: ValuationMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IrrProjections {
    #[serde(rename = "1yr")]
    pub one_year: f64,
    #[serde(rename = "2yr")]
    pub two_year: f64,
    #[serde(rename = "3yr")]
    pub three_year: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergerOutput {
    pub acquirer_id: String,
    pub target_id: String,
    pub valuation_methods: ValuationMethods,
    pub recommended_offer_price: f64,
    pub offer_premium: f64,
    pub total_deal_value: f64,
    pub synergy_value: f64,
    pub eps_impact: f64,
    pub accretive_dilutive: String,
    pub irr_projections: IrrProjections,
    pub recommendations: Vec<String>,
}

pub fn value_merger(request: &MergerRequest) -> MergerOutput {
    let target_dcf = request.target_equity * 1.1;
    let target_comps = request.target_equity * (1.0 + request.deal_premium);
    let target_asset = request.target_equity * 0.9;

    let combined_equity = request.acquirer_equity + request.target_equity;

    let offer_price = request.target_equity * (1.0 + request.deal_premium);
    let total_deal = offer_price + request.transaction_costs;

    let pe_implied = safe_div(offer_price, request.target_ebitda);

    let combined_eps = safe_div(
        combined_equity,
        request.acquirer_revenue + request.target_revenue,
    );
    let current_eps = safe_div(request.acquirer_equity, request.acquirer_revenue);
    let eps_impact = combined_eps - current_eps;
    let accretive = if eps_impact > 0.0 {
        "Accretive"
    } else {
        "Dilutive"
    };

    let ratio = safe_div(target_comps, offer_price) - 1.0;
    let irr_1yr = ratio * 100.0;
    let irr_2yr = ratio * 50.0;
    let irr_3yr = ratio * 33.33;

    let synergy_value = request.synergies_ebitda * safe_div(target_comps, request.target_ebitda);

    let mut recommendations = Vec::new();
    if pe_implied < request.acquirer_pe * 1.2 {
        recommendations.push("Deal appears accretive - proceed with valuation".to_string());
    }
    if request.deal_premium > 0.4 {
        recommendations
            .push("High premium requested - ensure synergy case supports valuation".to_string());
    }
    if irr_1yr < 15.0 {
        recommendations.push("Consider negotiating lower price to improve returns".to_string());
    }
    if request.integration_costs > total_deal * 0.1 {
        recommendations
            .push("Integration costs are significant - model full cost impact".to_string());
    }

    MergerOutput {
        acquirer_id: request.acquirer_id.clone(),
        target_id: request.target_id.clone(),
        valuation_methods: ValuationMethods {
            dcf: ValuationMethod {
                value: r2(target_dcf),
                weight: 0.4,
            },
            comparables: ValuationMethod {
                value: r2(target_comps),
                weight: 0.4,
            },
            asset: ValuationMethod {
                value: r2(target_asset),
                weight: 0.2,
            },
        },
        recommended_offer_price: r2(offer_price),
        offer_premium: r2(request.deal_premium * 100.0),
        total_deal_value: r2(total_deal),
        synergy_value: r2(synergy_value),
        eps_impact: r4(eps_impact),
        accretive_dilutive: accretive.to_string(),
        irr_projections: IrrProjections {
            one_year: r2(irr_1yr),
            two_year: r2(irr_2yr),
            three_year: r2(irr_3yr),
        },
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> MergerRequest {
        MergerRequest {
            acquirer_id: "ACQ".to_string(),
            target_id: "TGT".to_string(),
            acquirer_equity: 1000.0,
            target_equity: 400.0,
            acquirer_ebitda: 150.0,
            target_ebitda: 50.0,
            acquirer_revenue: 900.0,
            target_revenue: 300.0,
            synergies_ebitda: 20.0,
            acquirer_pe: 18.0,
            target_pe: 12.0,
            deal_premium: 0.25,
            transaction_costs: 15.0,
            integration_costs: 30.0,
        }
    }

    #[test]
    fn matches_the_service_valuation() {
        let out = value_merger(&request());
        assert_eq!(out.valuation_methods.dcf.value, 440.0);
        assert_eq!(out.valuation_methods.comparables.value, 500.0);
        assert_eq!(out.valuation_methods.asset.value, 360.0);
        assert_eq!(out.recommended_offer_price, 500.0);
        assert_eq!(out.offer_premium, 25.0);
        assert_eq!(out.total_deal_value, 515.0);
        assert_eq!(out.synergy_value, 200.0);
        assert_eq!(out.eps_impact, 0.0556);
        assert_eq!(out.accretive_dilutive, "Accretive");
        assert_eq!(out.irr_projections.one_year, 0.0);
        assert_eq!(out.irr_projections.two_year, 0.0);
        assert_eq!(out.irr_projections.three_year, 0.0);
    }

    #[test]
    fn recommendations_follow_the_service_rules() {
        let out = value_merger(&request());
        assert_eq!(
            out.recommendations,
            vec![
                "Deal appears accretive - proceed with valuation".to_string(),
                "Consider negotiating lower price to improve returns".to_string(),
            ]
        );

        let mut high = request();
        high.deal_premium = 0.5;
        high.integration_costs = 500.0;
        let out = value_merger(&high);
        assert!(out
            .recommendations
            .iter()
            .any(|r| r.contains("High premium requested")));
        assert!(out
            .recommendations
            .iter()
            .any(|r| r.contains("Integration costs are significant")));
        // The P/E test still passes at a 50% premium: 600 / 50 = 12 < 21.6.
        assert!(out
            .recommendations
            .iter()
            .any(|r| r == "Deal appears accretive - proceed with valuation"));
    }

    #[test]
    fn zero_target_ebitda_reports_zero_instead_of_dividing() {
        let mut req = request();
        req.target_ebitda = 0.0;
        let out = value_merger(&req);
        assert_eq!(out.synergy_value, 0.0);
    }
}
