//! forward-contracts-service: mark-to-market of a forward book.
//!
//! Python quirks carried over deliberately, because filed valuations use them:
//! time to delivery is fixed at half a year for every contract regardless of
//! its actual maturity, the theoretical forward applies one interest-rate
//! differential to the book's single spot rate rather than to each contract's
//! own, the "delta" is just the market value again, and the hedge ratio is
//! |net MtM| / notional, which is not a coverage measure at all.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vimbai_common::py_round;

/// Every contract is valued as if it matures in six months.
const TIME_TO_DELIVERY: f64 = 0.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardContract {
    pub contract_id: String,
    pub currency_pair: String,
    pub notional: f64,
    pub spot_rate: f64,
    pub forward_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardContractRequest {
    pub company_id: String,
    pub contracts: Vec<ForwardContract>,
    pub current_spot_rate: f64,
    pub risk_free_rate_domestic: f64,
    pub risk_free_rate_foreign: f64,
    #[serde(default)]
    pub volatility: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForwardContractAnalysis {
    pub contract_id: String,
    pub currency_pair: String,
    pub notional: f64,
    pub spot_rate: f64,
    pub forward_rate: f64,
    pub market_value: f64,
    pub market_value_pct: f64,
    pub theoretical_forward: f64,
    pub forward_points: f64,
    pub delta: f64,
    pub gamma_risk: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExposureSummary {
    pub long_contracts: usize,
    pub short_contracts: usize,
    pub total_exposure: f64,
    pub net_mtm: f64,
    pub hedge_ratio: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForwardContractResponse {
    pub company_id: String,
    pub portfolio_valuation: BTreeMap<String, f64>,
    pub contract_analysis: Vec<ForwardContractAnalysis>,
    pub exposure_summary: ExposureSummary,
    pub recommendations: Vec<String>,
}

pub fn analyze_forward_contracts(req: &ForwardContractRequest) -> ForwardContractResponse {
    let theoretical_forward = req.current_spot_rate
        * ((req.risk_free_rate_domestic - req.risk_free_rate_foreign) * TIME_TO_DELIVERY).exp();

    let mut contract_analysis = Vec::new();
    let mut total_mtm = 0.0;
    let mut total_exposure = 0.0;

    for contract in &req.contracts {
        let mtm = contract.notional * (contract.forward_rate - req.current_spot_rate);
        let mtm_pct = if contract.notional != 0.0 {
            mtm / contract.notional * 100.0
        } else {
            0.0
        };
        let sensitivity = contract.notional * TIME_TO_DELIVERY * req.volatility;

        contract_analysis.push(ForwardContractAnalysis {
            contract_id: contract.contract_id.clone(),
            currency_pair: contract.currency_pair.clone(),
            notional: contract.notional,
            spot_rate: contract.spot_rate,
            forward_rate: contract.forward_rate,
            market_value: py_round(mtm, 2),
            market_value_pct: py_round(mtm_pct, 4),
            theoretical_forward: py_round(theoretical_forward, 4),
            forward_points: py_round(contract.forward_rate - contract.spot_rate, 4),
            delta: py_round(mtm, 2),
            gamma_risk: py_round(sensitivity, 2),
        });

        total_mtm += mtm;
        total_exposure += contract.notional;
    }

    let long_contracts = contract_analysis
        .iter()
        .filter(|row| row.market_value > 0.0)
        .count();
    let short_contracts = contract_analysis
        .iter()
        .filter(|row| row.market_value < 0.0)
        .count();
    let hedge_ratio = if total_exposure != 0.0 {
        total_mtm.abs() / total_exposure
    } else {
        0.0
    };

    let mut recommendations = Vec::new();
    if total_mtm < -total_exposure * 0.1 {
        recommendations.push(
            "Significant unrealized loss - consider early termination or renegotiation".to_string(),
        );
    }
    if hedge_ratio < 0.5 {
        recommendations
            .push("Under-hedged position - consider adding forward contracts".to_string());
    }
    if hedge_ratio > 1.2 {
        recommendations
            .push("Over-hedged - reduce exposure to avoid speculative position".to_string());
    }

    let mut portfolio_valuation = BTreeMap::new();
    portfolio_valuation.insert("total_mtm".to_string(), py_round(total_mtm, 2));
    portfolio_valuation.insert("total_exposure".to_string(), total_exposure);

    ForwardContractResponse {
        company_id: req.company_id.clone(),
        portfolio_valuation,
        contract_analysis,
        exposure_summary: ExposureSummary {
            long_contracts,
            short_contracts,
            total_exposure,
            net_mtm: py_round(total_mtm, 2),
            hedge_ratio,
        },
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ForwardContractRequest {
        ForwardContractRequest {
            company_id: "c1".into(),
            contracts: vec![
                ForwardContract {
                    contract_id: "f1".into(),
                    currency_pair: "USD/ZWG".into(),
                    notional: 500_000.0,
                    spot_rate: 25.0,
                    forward_rate: 27.0,
                },
                ForwardContract {
                    contract_id: "f2".into(),
                    currency_pair: "USD/ZWG".into(),
                    notional: 250_000.0,
                    spot_rate: 26.0,
                    forward_rate: 25.5,
                },
            ],
            current_spot_rate: 26.5,
            risk_free_rate_domestic: 0.12,
            risk_free_rate_foreign: 0.05,
            volatility: 0.15,
        }
    }

    #[test]
    fn mark_to_market_uses_the_book_spot() {
        let out = analyze_forward_contracts(&request());
        assert_eq!(out.contract_analysis[0].market_value, 250_000.0);
        assert_eq!(out.contract_analysis[1].market_value, -250_000.0);
        assert_eq!(out.portfolio_valuation["total_mtm"], 0.0);
    }

    #[test]
    fn long_and_short_split_by_market_value() {
        let out = analyze_forward_contracts(&request());
        assert_eq!(out.exposure_summary.long_contracts, 1);
        assert_eq!(out.exposure_summary.short_contracts, 1);
        assert_eq!(out.exposure_summary.total_exposure, 750_000.0);
    }

    #[test]
    fn hedge_ratio_is_mtm_over_notional() {
        let mut req = request();
        req.contracts.pop();
        let out = analyze_forward_contracts(&req);
        assert_eq!(out.exposure_summary.net_mtm, 250_000.0);
        assert_eq!(out.exposure_summary.hedge_ratio, 0.5);
    }

    #[test]
    fn empty_book_does_not_divide_by_zero() {
        let mut req = request();
        req.contracts.clear();
        let out = analyze_forward_contracts(&req);
        assert_eq!(out.exposure_summary.hedge_ratio, 0.0);
        assert_eq!(out.exposure_summary.long_contracts, 0);
    }
}
