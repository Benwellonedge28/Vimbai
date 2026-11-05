//! Balanced scorecard: four perspectives, weighted achievement, and the actions
//! a shortfall should trigger.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use vimbai_common::numbers::round_dp;

/// The four perspectives, in the order the service emitted them.
pub const PERSPECTIVES: [&str; 4] = ["financial", "customer", "internal", "learning_growth"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kpi {
    pub name: String,
    /// One of [`PERSPECTIVES`]; anything else lands in financial.
    pub perspective: String,
    pub target: f64,
    pub actual: f64,
    pub weight: f64,
    pub unit: String,
}

impl Kpi {
    pub fn new(name: &str, perspective: &str, target: f64, actual: f64) -> Kpi {
        Kpi {
            name: name.to_string(),
            perspective: perspective.to_string(),
            target,
            actual,
            weight: 1.0,
            unit: "number".to_string(),
        }
    }

    pub fn weighted(mut self, weight: f64) -> Kpi {
        self.weight = weight;
        self
    }

    /// Achievement, capped at 100%: beating a target earns no extra credit and
    /// a zero or negative target is treated as already met.
    pub fn achievement(&self) -> f64 {
        if self.target > 0.0 {
            (self.actual / self.target).min(1.0) * 100.0
        } else {
            100.0
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScorecardRequest {
    pub company_id: String,
    pub period: String,
    pub kpis: Vec<Kpi>,
}

impl ScorecardRequest {
    pub fn new(company_id: &str, period: &str, kpis: Vec<Kpi>) -> ScorecardRequest {
        ScorecardRequest {
            company_id: company_id.to_string(),
            period: period.to_string(),
            kpis,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScorecardResult {
    pub company_id: String,
    pub period: String,
    pub financial_score: f64,
    pub customer_score: f64,
    pub internal_score: f64,
    pub learning_score: f64,
    pub overall_score: f64,
    pub perspectives: IndexMap<String, Value>,
    pub action_items: Vec<String>,
}

/// Score the period.
///
/// Preserved quirks, both visible in reported scores:
/// * a perspective's score is weighted achievement divided by total weight, then
///   multiplied by 100 again, so a perspective with one KPI at 80% scores 8,000;
/// * the overall score is the plain mean of the four perspectives, so a card with
///   twelve financial KPIs and one learning KPI weights them equally.
pub fn score(request: &ScorecardRequest) -> ScorecardResult {
    let mut buckets: IndexMap<String, (Vec<Value>, f64, f64)> = IndexMap::new();
    for perspective in PERSPECTIVES {
        buckets.insert(perspective.to_string(), (Vec::new(), 0.0, 0.0));
    }
    let mut action_items: Vec<String> = Vec::new();

    for kpi in &request.kpis {
        let key = if buckets.contains_key(&kpi.perspective) {
            kpi.perspective.clone()
        } else {
            "financial".to_string()
        };
        let achievement = kpi.achievement();
        let entry = buckets.get_mut(&key).expect("bucket exists");
        entry.0.push(json!({
            "name": kpi.name,
            "target": kpi.target,
            "actual": kpi.actual,
            "achievement_pct": round_dp(achievement, 1),
            "weight": kpi.weight,
        }));
        entry.1 += achievement * kpi.weight;
        entry.2 += kpi.weight;
        if achievement < 80.0 {
            action_items.push(format!(
                "{}: Below target ({:.0}%) - review strategy",
                kpi.name, achievement
            ));
        }
    }

    let mut perspectives: IndexMap<String, Value> = IndexMap::new();
    let mut scores: IndexMap<String, f64> = IndexMap::new();
    for perspective in PERSPECTIVES {
        let (kpis, weighted, weight_total) = buckets.get(perspective).cloned().unwrap_or_default();
        let score = if weight_total != 0.0 {
            round_dp(weighted / weight_total * 100.0, 1)
        } else {
            0.0
        };
        perspectives.insert(
            perspective.to_string(),
            json!({ "kpis": kpis, "score": score }),
        );
        scores.insert(perspective.to_string(), score);
    }

    let overall = round_dp(scores.values().sum::<f64>() / 4.0, 1);
    if overall < 70.0 {
        action_items.insert(
            0,
            "Overall performance below 70% - strategic realignment needed".to_string(),
        );
    } else if overall >= 90.0 {
        action_items.insert(
            0,
            "Excellent performance across all perspectives".to_string(),
        );
    }
    if action_items.is_empty() {
        action_items.push("All KPIs on track".to_string());
    }

    ScorecardResult {
        company_id: request.company_id.clone(),
        period: request.period.clone(),
        financial_score: scores["financial"],
        customer_score: scores["customer"],
        internal_score: scores["internal"],
        learning_score: scores["learning_growth"],
        overall_score: overall,
        perspectives,
        action_items,
    }
}
