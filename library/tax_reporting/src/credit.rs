//! R&D tax credit: regular method against alternative simplified.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use vimbai_common::numbers::money;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expenditure {
    pub category: String,
    pub description: String,
    pub amount: f64,
    pub qualifies: bool,
}

impl Expenditure {
    pub fn new(category: &str, amount: f64, qualifies: bool) -> Expenditure {
        Expenditure {
            category: category.to_string(),
            description: String::new(),
            amount,
            qualifies,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimRequest {
    pub company_id: String,
    pub fiscal_year: i32,
    pub expenditures: Vec<Expenditure>,
    pub credit_rate: f64,
    pub alternative_rate: f64,
}

impl ClaimRequest {
    pub fn new(company_id: &str, fiscal_year: i32, expenditures: Vec<Expenditure>) -> ClaimRequest {
        ClaimRequest {
            company_id: company_id.to_string(),
            fiscal_year,
            expenditures,
            credit_rate: 0.15,
            alternative_rate: 0.20,
        }
    }

    pub fn with_rates(mut self, credit_rate: f64, alternative_rate: f64) -> ClaimRequest {
        self.credit_rate = credit_rate;
        self.alternative_rate = alternative_rate;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClaimResult {
    pub company_id: String,
    pub fiscal_year: i32,
    pub total_expenditure: f64,
    pub qualifying_expenditure: f64,
    pub non_qualifying: f64,
    pub credit_regular: f64,
    pub credit_alternative: f64,
    pub recommended_method: String,
    pub expenditure_breakdown: IndexMap<String, f64>,
}

/// Compute a credit claim.
///
/// The recommendation keeps the service's tie-break: equal credits recommend
/// `alternative_simplified`, not `regular`.
pub fn calculate(request: &ClaimRequest) -> ClaimResult {
    let total: f64 = request.expenditures.iter().map(|e| e.amount).sum();
    let qualifying: f64 = request
        .expenditures
        .iter()
        .filter(|e| e.qualifies)
        .map(|e| e.amount)
        .sum();
    let credit_regular = qualifying * request.credit_rate;
    let credit_alternative = qualifying * request.alternative_rate;

    // Categories keep first-seen order, as a dict built in a loop would.
    let mut breakdown: IndexMap<String, f64> = IndexMap::new();
    for expenditure in request.expenditures.iter().filter(|e| e.qualifies) {
        *breakdown.entry(expenditure.category.clone()).or_insert(0.0) += expenditure.amount;
    }

    ClaimResult {
        company_id: request.company_id.clone(),
        fiscal_year: request.fiscal_year,
        total_expenditure: money(total),
        qualifying_expenditure: money(qualifying),
        non_qualifying: money(total - qualifying),
        credit_regular: money(credit_regular),
        credit_alternative: money(credit_alternative),
        recommended_method: if credit_regular > credit_alternative {
            "regular".to_string()
        } else {
            "alternative_simplified".to_string()
        },
        expenditure_breakdown: breakdown
            .into_iter()
            .map(|(category, amount)| (category, money(amount)))
            .collect(),
    }
}
