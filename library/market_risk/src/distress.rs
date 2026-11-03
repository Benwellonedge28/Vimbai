//! Altman-style Z-score and the zone it falls in.

use serde::{Deserialize, Serialize};

use crate::money::round_dp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanyData {
    pub company_id: String,
    pub company_name: String,
    pub industry: String,
    pub working_capital: f64,
    pub total_assets: f64,
    pub ebit: f64,
    pub total_equity: f64,
    pub total_liabilities: f64,
    pub revenue: f64,
    pub retained_earnings: f64,
    pub market_cap: f64,
    pub current_assets: f64,
    pub current_liabilities: f64,
}

impl CompanyData {
    pub fn new(company_id: &str, company_name: &str, total_assets: f64) -> CompanyData {
        CompanyData {
            company_id: company_id.to_string(),
            company_name: company_name.to_string(),
            industry: String::new(),
            working_capital: 0.0,
            total_assets,
            ebit: 0.0,
            total_equity: 0.0,
            total_liabilities: 0.0,
            revenue: 0.0,
            retained_earnings: 0.0,
            market_cap: 0.0,
            current_assets: 0.0,
            current_liabilities: 0.0,
        }
    }

    pub fn in_industry(mut self, industry: &str) -> CompanyData {
        self.industry = industry.to_string();
        self
    }

    pub fn with_figures(
        mut self,
        working_capital: f64,
        retained_earnings: f64,
        ebit: f64,
        equity: f64,
        liabilities: f64,
        revenue: f64,
    ) -> CompanyData {
        self.working_capital = working_capital;
        self.retained_earnings = retained_earnings;
        self.ebit = ebit;
        self.total_equity = equity;
        self.total_liabilities = liabilities;
        self.revenue = revenue;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DistressPrediction {
    pub company_id: String,
    pub z_score: f64,
    pub z1: f64,
    pub z2: f64,
    pub z3: f64,
    pub z4: f64,
    pub z5: f64,
    pub zone: String,
    pub probability_distress: f64,
    pub years_to_distress: f64,
    pub recommendation: String,
}

/// Predict distress.
///
/// Preserved: the weights are the original Altman set applied to *book* equity
/// (the classic model uses market value for the fourth term), the thresholds
/// differ for manufacturing, and years-to-distress steps in half years that were
/// presented as a forecast even though they come from fixed bands.
pub fn predict(company: &CompanyData) -> DistressPrediction {
    let assets = company.total_assets;
    let z1 = if assets != 0.0 {
        company.working_capital / assets * 1.2
    } else {
        0.0
    };
    let z2 = if assets != 0.0 {
        company.retained_earnings / assets * 1.4
    } else {
        0.0
    };
    let z3 = if assets != 0.0 {
        company.ebit / assets * 3.3
    } else {
        0.0
    };
    let z4 = if company.total_liabilities != 0.0 {
        company.total_equity / company.total_liabilities * 0.6
    } else {
        0.0
    };
    let z5 = if assets != 0.0 {
        company.revenue / assets
    } else {
        0.0
    };
    let z_score = z1 + z2 + z3 + z4 + z5;

    let manufacturing = company.industry == "manufacturing";
    let (zone, probability) = if manufacturing {
        if z_score > 2.99 {
            ("SAFE", 0.03)
        } else if z_score > 1.81 {
            ("GREY", 0.35)
        } else {
            ("DISTRESS", 0.95)
        }
    } else if z_score > 2.6 {
        ("SAFE", 0.05)
    } else if z_score > 1.5 {
        ("GREY", 0.40)
    } else {
        ("DISTRESS", 0.90)
    };

    let (years, recommendation) = if z_score > 3.0 {
        (
            5.0,
            "Company is financially healthy. Continue monitoring and maintain current strategies.",
        )
    } else if z_score > 2.0 {
        (
            2.5,
            "Company shows signs of stress. Consider reviewing operations and improving liquidity.",
        )
    } else if z_score > 1.0 {
        (
            1.0,
            "High risk of distress. Urgent action needed - consider restructuring options.",
        )
    } else {
        (
            0.5,
            "Near-term bankruptcy risk. Seek professional financial advice immediately.",
        )
    };

    DistressPrediction {
        company_id: company.company_id.clone(),
        z_score: round_dp(z_score, 2),
        z1: round_dp(z1, 2),
        z2: round_dp(z2, 2),
        z3: round_dp(z3, 2),
        z4: round_dp(z4, 2),
        z5: round_dp(z5, 2),
        zone: zone.to_string(),
        probability_distress: probability,
        years_to_distress: years,
        recommendation: recommendation.to_string(),
    }
}
