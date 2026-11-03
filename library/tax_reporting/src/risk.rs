//! Tax risk register: weighted exposure and a banded level per risk.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::money::money;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn value(self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaxRiskItem {
    pub risk_id: String,
    pub description: String,
    /// `transfer_pricing`, `vat_gap`, `permanent_establishment`, `treaty`, `reporting`.
    pub risk_type: String,
    pub potential_exposure: f64,
    pub probability: f64,
    pub jurisdiction: String,
    pub mitigation_actions: Vec<String>,
}

impl TaxRiskItem {
    pub fn new(
        risk_id: &str,
        description: &str,
        risk_type: &str,
        exposure: f64,
        probability: f64,
    ) -> TaxRiskItem {
        TaxRiskItem {
            risk_id: risk_id.to_string(),
            description: description.to_string(),
            risk_type: risk_type.to_string(),
            potential_exposure: exposure,
            probability,
            jurisdiction: "ZW".to_string(),
            mitigation_actions: Vec::new(),
        }
    }

    pub fn in_jurisdiction(mut self, jurisdiction: &str) -> TaxRiskItem {
        self.jurisdiction = jurisdiction.to_string();
        self
    }

    pub fn mitigated_by(mut self, actions: &[&str]) -> TaxRiskItem {
        self.mitigation_actions = actions.iter().map(|a| a.to_string()).collect();
        self
    }

    fn weighted(&self) -> f64 {
        self.potential_exposure * self.probability
    }
}

/// Band a weighted exposure. Thresholds are the service's own.
pub fn overall_level(weighted_exposure: f64) -> RiskLevel {
    if weighted_exposure > 500_000.0 {
        RiskLevel::Critical
    } else if weighted_exposure > 100_000.0 {
        RiskLevel::High
    } else if weighted_exposure > 25_000.0 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

fn item_level(score: f64) -> RiskLevel {
    if score > 250_000.0 {
        RiskLevel::Critical
    } else if score > 50_000.0 {
        RiskLevel::High
    } else if score > 10_000.0 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TaxRiskAssessment {
    pub company_id: String,
    pub assessment_date: String,
    pub overall_risk_level: RiskLevel,
    pub total_exposure: f64,
    pub weighted_exposure: f64,
    pub risk_count: usize,
    pub risks: Vec<Value>,
    pub recommendations: Vec<String>,
}

/// Standing recommendations. The service returned the same five for every
/// assessment; they are kept here rather than invented per client.
pub fn default_recommendations() -> Vec<String> {
    vec![
        "Conduct regular transfer pricing documentation reviews".to_string(),
        "Implement real-time VAT reconciliation to minimize VAT gaps".to_string(),
        "Monitor permanent establishment risks in cross-border operations".to_string(),
        "Maintain up-to-date tax treaty position documentation".to_string(),
        "Engage external tax advisors for high-exposure areas".to_string(),
    ]
}

pub fn assess(company_id: &str, assessment_date: &str, risks: &[TaxRiskItem]) -> TaxRiskAssessment {
    let total_exposure: f64 = risks.iter().map(|r| r.potential_exposure).sum();
    let weighted: f64 = risks.iter().map(TaxRiskItem::weighted).sum();

    let detail = risks
        .iter()
        .map(|risk| {
            let score = risk.weighted();
            json!({
                "risk_id": risk.risk_id,
                "description": risk.description,
                "risk_type": risk.risk_type,
                "potential_exposure": risk.potential_exposure,
                "probability": risk.probability,
                "weighted_score": money(score),
                "level": item_level(score).value(),
                "jurisdiction": risk.jurisdiction,
                "mitigation_actions": risk.mitigation_actions,
            })
        })
        .collect();

    TaxRiskAssessment {
        company_id: company_id.to_string(),
        assessment_date: assessment_date.to_string(),
        overall_risk_level: overall_level(weighted),
        total_exposure: money(total_exposure),
        weighted_exposure: money(weighted),
        risk_count: risks.len(),
        risks: detail,
        recommendations: default_recommendations(),
    }
}
