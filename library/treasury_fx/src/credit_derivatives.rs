//! credit-derivatives-service: single-name CDS and CDS-index mark-to-market.
//!
//! Python quirks kept, because the numbers are quoted in filed reports: the
//! expected loss is a flat 1% of notional rather than a probability-weighted
//! figure, the CVA uses the same 1% and ignores the position's own spread, and
//! a CDS is valued as premium leg minus expected loss rather than as a spread
//! differential. Spreads are in basis points, hence the /10,000.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditPosition {
    pub position_id: String,
    pub instrument_type: String,
    pub reference_entity: String,
    pub notional: f64,
    pub spread: f64,
    #[serde(default)]
    pub rating: String,
    pub maturity_years: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditDerivativesRequest {
    pub company_id: String,
    pub positions: Vec<CreditPosition>,
    pub risk_free_rate: f64,
    pub recovery_rate: f64,
    #[serde(default)]
    pub credit_spreads: HashMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreditPositionValuation {
    pub position_id: String,
    pub instrument: String,
    pub reference_entity: String,
    pub notional: f64,
    pub current_spread: f64,
    pub market_spread: f64,
    pub spread_diff: f64,
    pub mtm: f64,
    pub mtm_pct: f64,
    pub rating: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreditDerivativesResponse {
    pub company_id: String,
    pub position_valuation: Vec<CreditPositionValuation>,
    pub portfolio_exposure: BTreeMap<String, f64>,
    pub cva_calculation: BTreeMap<String, f64>,
    pub recommendations: Vec<String>,
}

/// Flat 1% loss given the recovery assumption, applied to notional.
const EXPECTED_LOSS_RATE: f64 = 0.01;

pub fn analyze_credit_derivatives(req: &CreditDerivativesRequest) -> CreditDerivativesResponse {
    let mut position_valuation = Vec::new();
    let mut total_exposure = 0.0;
    let mut total_mtm = 0.0;
    let mut total_cva = 0.0;

    for position in &req.positions {
        let market_spread = req
            .credit_spreads
            .get(&position.reference_entity)
            .copied()
            .unwrap_or(position.spread);
        let spread_diff = position.spread - market_spread;
        let pv_factor = (-req.risk_free_rate * position.maturity_years).exp();
        let expected_loss = position.notional * (1.0 - req.recovery_rate) * EXPECTED_LOSS_RATE;

        let mtm = if position.instrument_type == "CDS" {
            let premium_leg = position.notional * position.spread / 10_000.0
                * position.maturity_years
                * pv_factor;
            let protection_leg = expected_loss * pv_factor;
            premium_leg - protection_leg
        } else {
            position.notional * spread_diff / 10_000.0 * pv_factor
        };

        let mtm_pct = if position.notional != 0.0 {
            mtm / position.notional * 100.0
        } else {
            0.0
        };

        position_valuation.push(CreditPositionValuation {
            position_id: position.position_id.clone(),
            instrument: position.instrument_type.clone(),
            reference_entity: position.reference_entity.clone(),
            notional: position.notional,
            current_spread: position.spread,
            market_spread: vimbai_common::py_round(market_spread, 2),
            spread_diff: vimbai_common::py_round(spread_diff, 2),
            mtm: vimbai_common::py_round(mtm, 2),
            mtm_pct: vimbai_common::py_round(mtm_pct, 4),
            rating: position.rating.clone(),
        });

        total_exposure += position.notional;
        total_mtm += mtm;
        total_cva += expected_loss * (-req.risk_free_rate * position.maturity_years).exp();
    }

    let avg_maturity = if req.positions.is_empty() {
        0.0
    } else {
        vimbai_common::py_round(
            req.positions.iter().map(|p| p.maturity_years).sum::<f64>()
                / req.positions.len() as f64,
            2,
        )
    };

    let mut portfolio_exposure = BTreeMap::new();
    portfolio_exposure.insert("total_notional".to_string(), total_exposure);
    portfolio_exposure.insert("net_mtm".to_string(), vimbai_common::py_round(total_mtm, 2));
    portfolio_exposure.insert("exposure_count".to_string(), req.positions.len() as f64);
    portfolio_exposure.insert("avg_maturity".to_string(), avg_maturity);

    let mut cva_calculation = BTreeMap::new();
    cva_calculation.insert(
        "total_cva".to_string(),
        vimbai_common::py_round(total_cva, 2),
    );
    cva_calculation.insert(
        "cva_pct_notional".to_string(),
        if total_exposure != 0.0 {
            vimbai_common::py_round(total_cva / total_exposure * 100.0, 4)
        } else {
            0.0
        },
    );
    cva_calculation.insert("recovery_assumption".to_string(), req.recovery_rate);

    let mut recommendations = Vec::new();
    if total_mtm < -total_exposure * 0.05 {
        recommendations
            .push("Significant negative MTM - review counterparty credit risk".to_string());
    }
    if avg_maturity > 5.0 {
        recommendations.push("Long-dated credit exposure - monitor spread volatility".to_string());
    }

    CreditDerivativesResponse {
        company_id: req.company_id.clone(),
        position_valuation,
        portfolio_exposure,
        cva_calculation,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> CreditDerivativesRequest {
        CreditDerivativesRequest {
            company_id: "c1".into(),
            positions: vec![
                CreditPosition {
                    position_id: "p1".into(),
                    instrument_type: "CDS".into(),
                    reference_entity: "Bank A".into(),
                    notional: 5_000_000.0,
                    spread: 250.0,
                    rating: "BBB".into(),
                    maturity_years: 5.0,
                },
                CreditPosition {
                    position_id: "p2".into(),
                    instrument_type: "CDS_INDEX".into(),
                    reference_entity: "CDX IG".into(),
                    notional: 2_000_000.0,
                    spread: 180.0,
                    rating: "A".into(),
                    maturity_years: 3.0,
                },
            ],
            risk_free_rate: 0.05,
            recovery_rate: 0.4,
            credit_spreads: HashMap::from([
                ("Bank A".to_string(), 220.0),
                ("CDX IG".to_string(), 200.0),
            ]),
        }
    }

    #[test]
    fn cds_is_premium_leg_minus_expected_loss() {
        let out = analyze_credit_derivatives(&request());
        let cds = &out.position_valuation[0];
        assert_eq!(cds.spread_diff, 30.0);
        // 5m * 250bp * 5y * e^-0.25  minus  5m * 0.6 * 1% * e^-0.25
        assert!(cds.mtm > 0.0);
        assert!(cds.mtm < 5_000_000.0);
    }

    #[test]
    fn index_uses_the_spread_differential_only() {
        let out = analyze_credit_derivatives(&request());
        let index = &out.position_valuation[1];
        // (180 - 200) is negative, so the index position is underwater
        assert!(index.mtm < 0.0);
        assert_eq!(index.spread_diff, -20.0);
    }

    #[test]
    fn cva_is_a_flat_one_percent_of_notional() {
        let out = analyze_credit_derivatives(&request());
        assert!(out.cva_calculation["total_cva"] > 0.0);
        assert!(out.cva_calculation["cva_pct_notional"] > 0.0);
        assert_eq!(out.cva_calculation["recovery_assumption"], 0.4);
    }

    #[test]
    fn long_dated_book_earns_the_monitoring_advice() {
        let mut req = request();
        for position in &mut req.positions {
            position.maturity_years = 7.0;
        }
        let out = analyze_credit_derivatives(&req);
        assert!(out
            .recommendations
            .contains(&"Long-dated credit exposure - monitor spread volatility".to_string()));
    }
}
