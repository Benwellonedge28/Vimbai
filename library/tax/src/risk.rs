//! tax-risk-service `/assess`: exposure weighting and risk bands.

use serde::{Deserialize, Serialize};
use vimbai_common::py_round2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRisk {
    pub risk_id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub risk_type: String,
    pub potential_exposure: f64,
    pub probability: f64,
    #[serde(default)]
    pub jurisdiction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRiskRequest {
    pub company_id: String,
    pub risks: Vec<TaxRisk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskDetail {
    pub risk_id: String,
    pub description: String,
    pub risk_type: String,
    pub potential_exposure: f64,
    pub probability: f64,
    pub weighted_score: f64,
    pub level: String,
    pub jurisdiction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRiskAssessment {
    pub company_id: String,
    pub overall_risk_level: String,
    pub total_exposure: f64,
    pub weighted_exposure: f64,
    pub risk_count: usize,
    pub risks: Vec<RiskDetail>,
    pub recommendations: Vec<String>,
}

pub fn assess_tax_risk(req: &TaxRiskRequest) -> TaxRiskAssessment {
    let total_exposure: f64 = req.risks.iter().map(|r| r.potential_exposure).sum();
    let weighted: f64 = req
        .risks
        .iter()
        .map(|r| r.potential_exposure * r.probability)
        .sum();

    let overall = if weighted > 500_000.0 {
        RiskLevel::Critical
    } else if weighted > 100_000.0 {
        RiskLevel::High
    } else if weighted > 25_000.0 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };

    let details = req
        .risks
        .iter()
        .map(|r| {
            let score = r.potential_exposure * r.probability;
            let level = if score > 250_000.0 {
                RiskLevel::Critical
            } else if score > 50_000.0 {
                RiskLevel::High
            } else if score > 10_000.0 {
                RiskLevel::Medium
            } else {
                RiskLevel::Low
            };
            RiskDetail {
                risk_id: r.risk_id.clone(),
                description: r.description.clone(),
                risk_type: r.risk_type.clone(),
                potential_exposure: r.potential_exposure,
                probability: r.probability,
                weighted_score: py_round2(score),
                level: level.as_str().to_string(),
                jurisdiction: r.jurisdiction.clone(),
            }
        })
        .collect();

    let recommendations = vec![
        "Conduct regular transfer pricing documentation reviews".to_string(),
        "Implement real-time VAT reconciliation to minimize VAT gaps".to_string(),
        "Monitor permanent establishment risks in cross-border operations".to_string(),
        "Maintain up-to-date tax treaty position documentation".to_string(),
        "Engage external tax advisors for high-exposure areas".to_string(),
    ];

    TaxRiskAssessment {
        company_id: req.company_id.clone(),
        overall_risk_level: overall.as_str().to_string(),
        total_exposure: py_round2(total_exposure),
        weighted_exposure: py_round2(weighted),
        risk_count: req.risks.len(),
        risks: details,
        recommendations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn risk(id: &str, exposure: f64, probability: f64) -> TaxRisk {
        TaxRisk {
            risk_id: id.into(),
            description: format!("risk {id}"),
            risk_type: "vat".into(),
            potential_exposure: exposure,
            probability,
            jurisdiction: "ZW".into(),
        }
    }

    #[test]
    fn overall_bands() {
        let low = assess_tax_risk(&TaxRiskRequest {
            company_id: "c".into(),
            risks: vec![risk("r1", 100_000.0, 0.1)],
        });
        assert_eq!(low.weighted_exposure, 10_000.0);
        assert_eq!(low.overall_risk_level, "low");

        let med = assess_tax_risk(&TaxRiskRequest {
            company_id: "c".into(),
            risks: vec![risk("r1", 200_000.0, 0.2)],
        });
        assert_eq!(med.overall_risk_level, "medium");

        let high = assess_tax_risk(&TaxRiskRequest {
            company_id: "c".into(),
            risks: vec![risk("r1", 1_000_000.0, 0.15)],
        });
        assert_eq!(high.overall_risk_level, "high");

        let crit = assess_tax_risk(&TaxRiskRequest {
            company_id: "c".into(),
            risks: vec![risk("r1", 1_000_000.0, 0.6)],
        });
        assert_eq!(crit.overall_risk_level, "critical");
    }

    #[test]
    fn per_risk_levels_and_totals() {
        let r = assess_tax_risk(&TaxRiskRequest {
            company_id: "c".into(),
            risks: vec![risk("r1", 600_000.0, 0.5), risk("r2", 50_000.0, 0.3)],
        });
        assert_eq!(r.risk_count, 2);
        assert_eq!(r.total_exposure, 650_000.0);
        assert_eq!(r.weighted_exposure, 315_000.0);
        assert_eq!(r.risks[0].level, "critical"); // 300_000 > 250_000
        assert_eq!(r.risks[1].level, "medium"); // 15_000 in (10_000, 50_000]
    }

    #[test]
    fn empty_risk_set_is_low() {
        let r = assess_tax_risk(&TaxRiskRequest {
            company_id: "c".into(),
            risks: vec![],
        });
        assert_eq!(r.overall_risk_level, "low");
        assert_eq!(r.weighted_exposure, 0.0);
        assert_eq!(r.recommendations.len(), 5);
    }
}
