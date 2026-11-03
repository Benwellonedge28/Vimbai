//! Regulatory capital, leverage and liquidity ratios.

use serde::{Deserialize, Serialize};

use crate::money::round_dp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegulatoryRatio {
    pub ratio_name: String,
    pub actual_value: f64,
    pub minimum_required: f64,
    pub buffer: f64,
    pub compliant: bool,
}

impl RegulatoryRatio {
    fn new(name: &str, actual: f64, minimum: f64, places: usize) -> RegulatoryRatio {
        RegulatoryRatio {
            ratio_name: name.to_string(),
            actual_value: round_dp(actual, places),
            minimum_required: minimum,
            buffer: round_dp(actual - minimum, places),
            compliant: actual >= minimum,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegulatoryRequest {
    pub company_id: String,
    pub regulator_type: String,
    pub tier1_capital: f64,
    pub tier2_capital: f64,
    pub risk_weighted_assets: f64,
    pub total_capital: f64,
    pub total_assets: f64,
    pub liquid_assets: f64,
    pub net_loan: f64,
}

impl RegulatoryRequest {
    pub fn new(company_id: &str, regulator_type: &str) -> RegulatoryRequest {
        RegulatoryRequest {
            company_id: company_id.to_string(),
            regulator_type: regulator_type.to_string(),
            tier1_capital: 0.0,
            tier2_capital: 0.0,
            risk_weighted_assets: 0.0,
            total_capital: 0.0,
            total_assets: 0.0,
            liquid_assets: 0.0,
            net_loan: 0.0,
        }
    }

    pub fn with_capital(
        mut self,
        tier1: f64,
        tier2: f64,
        total: f64,
        risk_weighted: f64,
    ) -> RegulatoryRequest {
        self.tier1_capital = tier1;
        self.tier2_capital = tier2;
        self.total_capital = total;
        self.risk_weighted_assets = risk_weighted;
        self
    }

    pub fn with_balance_sheet(
        mut self,
        total_assets: f64,
        liquid_assets: f64,
        net_loan: f64,
    ) -> RegulatoryRequest {
        self.total_assets = total_assets;
        self.liquid_assets = liquid_assets;
        self.net_loan = net_loan;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegulatoryResponse {
    pub company_id: String,
    pub regulator_type: String,
    pub capital_ratios: Vec<RegulatoryRatio>,
    pub liquidity_ratios: Vec<RegulatoryRatio>,
    pub leverage_ratios: Vec<RegulatoryRatio>,
    pub overall_compliance: String,
    pub recommendations: Vec<String>,
}

/// Calculate the ratios.
///
/// Preserved quirks: the CET1 row is measured with Tier 1 capital, because no
/// common-equity figure was ever an input, so CET1 and Tier 1 always report the
/// same value against different minimums; and `regulatory_capital` was accepted
/// and ignored. Minimums are the service's own Basel-shaped constants, not a
/// regulator's published requirement for any particular jurisdiction.
pub fn calculate(request: &RegulatoryRequest) -> RegulatoryResponse {
    let tier1_ratio = if request.risk_weighted_assets != 0.0 {
        request.tier1_capital / request.risk_weighted_assets
    } else {
        0.0
    };
    let total_cap_ratio = if request.risk_weighted_assets != 0.0 {
        request.total_capital / request.risk_weighted_assets
    } else {
        0.0
    };
    let leverage_ratio = if request.total_assets != 0.0 {
        request.tier1_capital / request.total_assets
    } else {
        0.0
    };
    let liquidity_ratio = if request.net_loan != 0.0 {
        request.liquid_assets / request.net_loan
    } else {
        0.0
    };

    let capital_ratios = vec![
        RegulatoryRatio::new("CET1 Ratio", tier1_ratio, 0.045, 4),
        RegulatoryRatio::new("Tier 1 Ratio", tier1_ratio, 0.06, 4),
        RegulatoryRatio::new("Total Capital Ratio", total_cap_ratio, 0.08, 4),
    ];
    let liquidity_ratios = vec![RegulatoryRatio::new(
        "Liquidity Coverage Ratio",
        liquidity_ratio,
        1.0,
        2,
    )];
    let leverage_ratios = vec![RegulatoryRatio::new(
        "Leverage Ratio",
        leverage_ratio,
        0.03,
        4,
    )];

    let all_compliant = capital_ratios
        .iter()
        .chain(liquidity_ratios.iter())
        .chain(leverage_ratios.iter())
        .all(|r| r.compliant);

    RegulatoryResponse {
        company_id: request.company_id.clone(),
        regulator_type: request.regulator_type.clone(),
        capital_ratios,
        liquidity_ratios,
        leverage_ratios,
        overall_compliance: if all_compliant {
            "compliant".to_string()
        } else {
            "non_compliant".to_string()
        },
        recommendations: if all_compliant {
            vec!["Continue monitoring regulatory ratios".to_string()]
        } else {
            vec![
                "Maintain capital buffers above minimum requirements".to_string(),
                "Monitor liquidity ratios regularly".to_string(),
                "Review risk-weighted assets calculation".to_string(),
            ]
        },
    }
}
