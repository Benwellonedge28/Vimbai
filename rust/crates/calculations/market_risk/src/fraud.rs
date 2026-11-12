//! Fraud risk: the three legs of the fraud triangle, weighted into one score.

use serde::Serialize;
use serde_json::{json, Value};

use vimbai_common::numbers::{money, python_float};

#[derive(Debug, Clone, PartialEq)]
pub struct FraudAssessmentRequest {
    pub audit_id: String,
    pub company_id: String,
    pub industry: String,
    pub company_size: String,
    pub financial_indicators: Vec<(String, f64)>,
    pub management_characteristics: Vec<String>,
    pub internal_control_gaps: Vec<String>,
    pub prior_fraud_history: bool,
}

impl FraudAssessmentRequest {
    pub fn new(audit_id: &str, company_id: &str) -> FraudAssessmentRequest {
        FraudAssessmentRequest {
            audit_id: audit_id.to_string(),
            company_id: company_id.to_string(),
            industry: String::new(),
            company_size: String::new(),
            financial_indicators: Vec::new(),
            management_characteristics: Vec::new(),
            internal_control_gaps: Vec::new(),
            prior_fraud_history: false,
        }
    }

    pub fn small_company(mut self) -> FraudAssessmentRequest {
        self.company_size = "small".to_string();
        self
    }

    pub fn with_history(mut self) -> FraudAssessmentRequest {
        self.prior_fraud_history = true;
        self
    }

    pub fn management(mut self, characteristics: &[&str]) -> FraudAssessmentRequest {
        self.management_characteristics = characteristics.iter().map(|c| c.to_string()).collect();
        self
    }

    pub fn control_gaps(mut self, gaps: &[&str]) -> FraudAssessmentRequest {
        self.internal_control_gaps = gaps.iter().map(|g| g.to_string()).collect();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FraudAssessment {
    pub audit_id: String,
    pub fraud_risk_score: f64,
    pub risk_category: String,
    pub pressure_indicators: Vec<Value>,
    pub opportunity_indicators: Vec<Value>,
    pub rationalization_indicators: Vec<Value>,
    pub high_risk_areas: Vec<String>,
    pub recommended_procedures: Vec<String>,
    pub conclusion: String,
}

fn indicator(category: &str, indicator: &str, risk_level: &str, weight: f64) -> Value {
    json!({
        "category": category,
        "indicator": indicator,
        "risk_level": risk_level,
        "weight": weight,
    })
}

/// Assess fraud risk.
///
/// Preserved: `financial_indicators` and `industry` are accepted and never used,
/// so a request carrying alarming ratios scores the same as one with none. Only
/// prior history, a small company, management turnover, control gaps and
/// aggressive targets move the score. The weights are the service's own.
pub fn assess(request: &FraudAssessmentRequest) -> FraudAssessment {
    let mut pressure = Vec::new();
    let mut opportunity = Vec::new();
    let mut rationalization = Vec::new();
    let (mut pressure_score, mut opportunity_score, mut rationalization_score) = (0.0, 0.0, 0.0);

    if request.prior_fraud_history {
        pressure.push(indicator("pressure", "Prior fraud incidents", "high", 0.3));
        pressure_score += 0.3;
    }
    if request.company_size == "small" {
        pressure.push(indicator("pressure", "Limited resources", "medium", 0.2));
        pressure_score += 0.2;
    }
    if request
        .management_characteristics
        .iter()
        .any(|c| c == "high_turnover")
    {
        pressure.push(indicator("pressure", "Management turnover", "high", 0.25));
        pressure_score += 0.25;
    }
    if request.internal_control_gaps.len() > 3 {
        opportunity.push(indicator(
            "opportunity",
            "Multiple control deficiencies",
            "high",
            0.35,
        ));
        opportunity_score += 0.35;
    }
    if request
        .internal_control_gaps
        .iter()
        .any(|g| g == "segregation_of_duties")
    {
        opportunity.push(indicator(
            "opportunity",
            "Inadequate segregation",
            "high",
            0.3,
        ));
        opportunity_score += 0.3;
    }
    if request
        .management_characteristics
        .iter()
        .any(|c| c == "aggressive_targets")
    {
        rationalization.push(indicator(
            "rationalization",
            "Aggressive targets",
            "medium",
            0.2,
        ));
        rationalization_score += 0.2;
    }

    let total_score =
        (pressure_score * 0.4 + opportunity_score * 0.4 + rationalization_score * 0.2) * 100.0;
    let risk_category = if total_score >= 70.0 {
        "high"
    } else if total_score >= 40.0 {
        "medium"
    } else {
        "low"
    };

    let mut high_risk_areas = Vec::new();
    if opportunity_score > 0.3 {
        high_risk_areas.push("Revenue recognition".to_string());
        high_risk_areas.push("Management override".to_string());
    }
    if pressure_score > 0.3 {
        high_risk_areas.push("Financial position manipulation".to_string());
    }
    if rationalization_score > 0.2 {
        high_risk_areas.push("Expense capitalization".to_string());
    }

    FraudAssessment {
        audit_id: request.audit_id.clone(),
        fraud_risk_score: money(total_score),
        risk_category: risk_category.to_string(),
        pressure_indicators: if pressure.is_empty() {
            vec![indicator(
                "pressure",
                "No significant pressures identified",
                "low",
                0.0,
            )]
        } else {
            pressure
        },
        opportunity_indicators: if opportunity.is_empty() {
            vec![indicator(
                "opportunity",
                "Controls appear adequate",
                "low",
                0.0,
            )]
        } else {
            opportunity
        },
        rationalization_indicators: if rationalization.is_empty() {
            vec![indicator(
                "rationalization",
                "No concerning rationalization",
                "low",
                0.0,
            )]
        } else {
            rationalization
        },
        high_risk_areas: if high_risk_areas.is_empty() {
            vec!["No high risk areas identified".to_string()]
        } else {
            high_risk_areas
        },
        recommended_procedures: if risk_category == "high" {
            vec![
                "Enhanced management inquiry".to_string(),
                "Journal entry testing".to_string(),
                "Significant estimates review".to_string(),
                "Related party transaction testing".to_string(),
            ]
        } else {
            vec!["Standard substantive procedures".to_string()]
        },
        conclusion: format!(
            "Fraud risk assessed as {risk_category} with score of {}",
            python_float(money(total_score))
        ),
    }
}
