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
use vimbai_common::{py_round, Money};

fn r2(value: f64) -> f64 {
    py_round(value, 2)
}

fn rate(value: f64) -> Money {
    Money::from_f64_shortest(value).unwrap_or_else(|_| Money::zero())
}

fn r4(value: f64) -> f64 {
    py_round(value, 4)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergerRequest {
    pub acquirer_id: String,
    pub target_id: String,
    pub acquirer_equity: Money,
    pub target_equity: Money,
    pub acquirer_ebitda: Money,
    pub target_ebitda: Money,
    pub acquirer_revenue: Money,
    pub target_revenue: Money,
    pub synergies_ebitda: Money,
    pub acquirer_pe: f64,
    pub target_pe: f64,
    pub deal_premium: f64,
    pub transaction_costs: Money,
    pub integration_costs: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValuationMethod {
    pub value: Money,
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
    pub recommended_offer_price: Money,
    pub offer_premium: f64,
    pub total_deal_value: Money,
    pub synergy_value: Money,
    pub eps_impact: f64,
    pub accretive_dilutive: String,
    pub irr_projections: IrrProjections,
    pub recommendations: Vec<String>,
}

pub fn value_merger(request: &MergerRequest) -> MergerOutput {
    let target_dcf = request.target_equity.times(&rate(1.1)).round(2);
    let target_comps = request
        .target_equity
        .times(&rate(1.0 + request.deal_premium))
        .round(2);
    let target_asset = request.target_equity.times(&rate(0.9)).round(2);

    let combined_equity = request.acquirer_equity.plus(&request.target_equity);
    let combined_revenue = request.acquirer_revenue.plus(&request.target_revenue);

    let offer_price = target_comps.clone();
    let total_deal = offer_price.plus(&request.transaction_costs);

    // A P/E is money over money, so it is a ratio, not an amount.
    let pe_implied = if request.target_ebitda.is_zero() {
        0.0
    } else {
        offer_price.over(&request.target_ebitda).to_f64_lossy()
    };

    let combined_eps = if combined_revenue.is_zero() {
        0.0
    } else {
        combined_equity.over(&combined_revenue).to_f64_lossy()
    };
    let current_eps = if request.acquirer_revenue.is_zero() {
        0.0
    } else {
        request
            .acquirer_equity
            .over(&request.acquirer_revenue)
            .to_f64_lossy()
    };
    let eps_impact = combined_eps - current_eps;
    let accretive = if eps_impact > 0.0 {
        "Accretive"
    } else {
        "Dilutive"
    };

    let ratio = if offer_price.is_zero() {
        0.0
    } else {
        target_comps.over(&offer_price).to_f64_lossy() - 1.0
    };
    let irr_1yr = ratio * 100.0;
    let irr_2yr = ratio * 50.0;
    let irr_3yr = ratio * 33.33;

    let comps_multiple = if request.target_ebitda.is_zero() {
        0.0
    } else {
        target_comps.over(&request.target_ebitda).to_f64_lossy()
    };
    let synergy_value = request
        .synergies_ebitda
        .times(&rate(comps_multiple))
        .round(2);

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
    if request.integration_costs > total_deal.times(&rate(0.1)) {
        recommendations
            .push("Integration costs are significant - model full cost impact".to_string());
    }

    MergerOutput {
        acquirer_id: request.acquirer_id.clone(),
        target_id: request.target_id.clone(),
        valuation_methods: ValuationMethods {
            dcf: ValuationMethod {
                value: target_dcf,
                weight: 0.4,
            },
            comparables: ValuationMethod {
                value: target_comps,
                weight: 0.4,
            },
            asset: ValuationMethod {
                value: target_asset,
                weight: 0.2,
            },
        },
        recommended_offer_price: offer_price,
        offer_premium: r2(request.deal_premium * 100.0),
        total_deal_value: total_deal,
        synergy_value,
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

/// `merger-valuation-service /value`.
pub fn merger_from_json(payload: &serde_json::Value) -> Result<serde_json::Value, String> {
    let request: MergerRequest = serde_json::from_value(payload.clone())
        .map_err(|error| format!("invalid request: {error}"))?;
    serde_json::to_value(value_merger(&request)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(text: &str) -> Money {
        Money::parse(text).expect("amount")
    }

    fn fixed(amount: &Money) -> String {
        amount.to_fixed(2)
    }

    fn request() -> MergerRequest {
        MergerRequest {
            acquirer_id: "ACQ".to_string(),
            target_id: "TGT".to_string(),
            acquirer_equity: m("1000"),
            target_equity: m("400"),
            acquirer_ebitda: m("150"),
            target_ebitda: m("50"),
            acquirer_revenue: m("900"),
            target_revenue: m("300"),
            synergies_ebitda: m("20"),
            acquirer_pe: 18.0,
            target_pe: 12.0,
            deal_premium: 0.25,
            transaction_costs: m("15"),
            integration_costs: m("30"),
        }
    }

    #[test]
    fn matches_the_service_valuation() {
        let out = value_merger(&request());
        assert_eq!(fixed(&out.valuation_methods.dcf.value), "440.00");
        assert_eq!(fixed(&out.valuation_methods.comparables.value), "500.00");
        assert_eq!(fixed(&out.valuation_methods.asset.value), "360.00");
        assert_eq!(fixed(&out.recommended_offer_price), "500.00");
        assert_eq!(out.offer_premium, 25.0);
        assert_eq!(fixed(&out.total_deal_value), "515.00");
        assert_eq!(fixed(&out.synergy_value), "200.00");
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
        high.integration_costs = m("500");
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
        req.target_ebitda = m("0");
        let out = value_merger(&req);
        assert_eq!(fixed(&out.synergy_value), "0.00");
    }

    #[test]
    fn an_equity_past_double_precision_multiplies_exactly() {
        let mut req = request();
        req.target_equity = m("123456789012345678901.55");
        req.deal_premium = 0.1;
        req.transaction_costs = m("0.45");
        let out = value_merger(&req);
        // 1.1x of a figure no double can hold, digit for digit
        assert_eq!(
            fixed(&out.valuation_methods.dcf.value),
            "135802467913580246791.70"
        );
        // 1.1x again for comparables, then the 0.45 of costs on top
        assert_eq!(
            fixed(&out.recommended_offer_price),
            "135802467913580246791.70"
        );
        assert_eq!(fixed(&out.total_deal_value), "135802467913580246792.15");
    }

    #[test]
    fn the_door_reads_the_published_shape() {
        let payload = serde_json::json!({
            "acquirer_id": "ACQ", "target_id": "TGT",
            "acquirer_equity": "1000", "target_equity": "400",
            "acquirer_ebitda": "150", "target_ebitda": "50",
            "acquirer_revenue": "900", "target_revenue": "300",
            "synergies_ebitda": "20", "acquirer_pe": 18.0, "target_pe": 12.0,
            "deal_premium": 0.25, "transaction_costs": "15",
            "integration_costs": "30"
        });
        let out = merger_from_json(&payload).expect("door");
        assert_eq!(out["recommended_offer_price"], "500.00");
        assert_eq!(out["synergy_value"], "200.00");
        assert_eq!(out["accretive_dilutive"], "Accretive");

        let bad = serde_json::json!({"acquirer_id": "ACQ", "target_equity": "not money"});
        let error = merger_from_json(&bad).expect_err("refused");
        assert!(error.contains("invalid request"), "{error}");
    }
}
