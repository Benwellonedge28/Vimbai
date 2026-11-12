//! Government grants: recognition, amortisation, and the entries to post.

use serde::Serialize;

use vimbai_common::numbers::{money, python_float};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantType {
    Asset,
    Income,
}

impl GrantType {
    pub fn value(self) -> &'static str {
        match self {
            GrantType::Asset => "asset",
            GrantType::Income => "income",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantRecognition {
    DeferredIncome,
    DeductionFromAsset,
}

impl GrantRecognition {
    pub fn value(self) -> &'static str {
        match self {
            GrantRecognition::DeferredIncome => "deferred_income",
            GrantRecognition::DeductionFromAsset => "deduction_from_asset",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrantRequest {
    pub company_id: String,
    pub grant_name: String,
    pub granting_authority: String,
    pub grant_amount: f64,
    pub grant_type: GrantType,
    pub recognition_method: GrantRecognition,
    pub asset_useful_life: i32,
    pub related_asset_cost: f64,
    pub conditions: Vec<String>,
    pub compliance_status: String,
}

impl GrantRequest {
    pub fn new(
        company_id: &str,
        grant_name: &str,
        grant_amount: f64,
        grant_type: GrantType,
    ) -> GrantRequest {
        GrantRequest {
            company_id: company_id.to_string(),
            grant_name: grant_name.to_string(),
            granting_authority: String::new(),
            grant_amount,
            grant_type,
            recognition_method: GrantRecognition::DeferredIncome,
            asset_useful_life: 5,
            related_asset_cost: 0.0,
            conditions: Vec::new(),
            compliance_status: "compliant".to_string(),
        }
    }

    pub fn from_authority(mut self, authority: &str) -> GrantRequest {
        self.granting_authority = authority.to_string();
        self
    }

    pub fn over_life(mut self, years: i32) -> GrantRequest {
        self.asset_useful_life = years;
        self
    }

    pub fn against_asset(mut self, cost: f64) -> GrantRequest {
        self.related_asset_cost = cost;
        self
    }

    pub fn on_conditions(mut self, conditions: &[&str]) -> GrantRequest {
        self.conditions = conditions.iter().map(|c| c.to_string()).collect();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GrantResult {
    pub company_id: String,
    pub grant_name: String,
    pub granting_authority: String,
    pub grant_amount: f64,
    pub grant_type: String,
    pub recognition_method: String,
    pub annual_recognition: f64,
    pub deferred_income_balance: f64,
    pub annual_amortization: f64,
    pub journal_entries: Vec<String>,
    pub disclosure_notes: Vec<String>,
    pub conditions: Vec<String>,
    pub compliance_status: String,
}

/// Recognise a grant in its first year.
///
/// Preserved quirk: an income grant is recognised in full and leaves no deferred
/// balance, yet its disclosure note still says "Recognized over 5 years", because
/// the note reads `asset_useful_life` regardless of grant type. The wording is
/// kept so filed comparatives do not shift.
pub fn recognize(request: &GrantRequest) -> GrantResult {
    let (annual, deferred) = match request.grant_type {
        GrantType::Asset => {
            let annual = if request.asset_useful_life != 0 {
                request.grant_amount / request.asset_useful_life as f64
            } else {
                0.0
            };
            (annual, request.grant_amount - annual)
        }
        GrantType::Income => (request.grant_amount, 0.0),
    };

    let conditions_text = if request.conditions.is_empty() {
        "None specified".to_string()
    } else {
        request.conditions.join(", ")
    };

    GrantResult {
        company_id: request.company_id.clone(),
        grant_name: request.grant_name.clone(),
        granting_authority: request.granting_authority.clone(),
        grant_amount: money(request.grant_amount),
        grant_type: request.grant_type.value().to_string(),
        recognition_method: request.recognition_method.value().to_string(),
        annual_recognition: money(annual),
        deferred_income_balance: money(deferred),
        annual_amortization: money(annual),
        journal_entries: vec![
            format!("Dr Bank {}", python_float(money(request.grant_amount))),
            format!(
                "Cr Deferred Income {}",
                python_float(money(request.grant_amount))
            ),
        ],
        disclosure_notes: vec![
            format!(
                "Grant received from {}",
                if request.granting_authority.is_empty() {
                    "government authority".to_string()
                } else {
                    request.granting_authority.clone()
                }
            ),
            format!("Recognized over {} years", request.asset_useful_life),
            format!("Conditions: {conditions_text}"),
        ],
        conditions: request.conditions.clone(),
        compliance_status: request.compliance_status.clone(),
    }
}
