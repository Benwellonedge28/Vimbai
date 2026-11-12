//! XBRL fact assembly and the five-concept validation an IFRS filing needs.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The concept list, the fact shape and the validation sentence are shared with the statement
/// filing in `vimbai_reporting::xbrl`; only this envelope is tax-specific.
pub use vimbai_reporting::xbrl::REQUIRED_CONCEPTS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinancialConcept {
    pub concept: String,
    pub value: f64,
    pub context_ref: String,
    pub unit_ref: String,
    pub decimals: i32,
}

impl FinancialConcept {
    pub fn new(concept: &str, value: f64) -> FinancialConcept {
        FinancialConcept {
            concept: concept.to_string(),
            value,
            context_ref: "current_year".to_string(),
            unit_ref: "USD".to_string(),
            decimals: -2,
        }
    }

    pub fn in_context(mut self, context_ref: &str) -> FinancialConcept {
        self.context_ref = context_ref.to_string();
        self
    }

    pub fn in_unit(mut self, unit_ref: &str) -> FinancialConcept {
        self.unit_ref = unit_ref.to_string();
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct XbrlRequest {
    pub company_id: String,
    pub fiscal_year: i32,
    pub entity_name: String,
    pub entity_identifier: String,
    pub taxonomy: String,
    pub concepts: Vec<FinancialConcept>,
}

impl XbrlRequest {
    pub fn new(
        company_id: &str,
        fiscal_year: i32,
        entity_name: &str,
        entity_identifier: &str,
    ) -> XbrlRequest {
        XbrlRequest {
            company_id: company_id.to_string(),
            fiscal_year,
            entity_name: entity_name.to_string(),
            entity_identifier: entity_identifier.to_string(),
            // `ifrs-full` or `us-gaap`
            taxonomy: "ifrs-full".to_string(),
            concepts: Vec::new(),
        }
    }

    pub fn with_taxonomy(mut self, taxonomy: &str) -> XbrlRequest {
        self.taxonomy = taxonomy.to_string();
        self
    }

    pub fn with_concepts(mut self, concepts: Vec<FinancialConcept>) -> XbrlRequest {
        self.concepts = concepts;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct XbrlResult {
    pub company_id: String,
    pub entity_name: String,
    pub fiscal_year: i32,
    pub taxonomy: String,
    pub concept_count: usize,
    pub validation_status: String,
    pub validation_errors: Vec<String>,
    pub xbrl_facts: Vec<Value>,
    pub filing_metadata: Value,
}

/// Validate and assemble facts.
///
/// The five required concepts are the ones a filing is rejected without, so the
/// check stays exactly where it was: a fact list missing any of them is
/// `invalid`, and the error names the concept.
pub fn generate(request: &XbrlRequest, generated_at: &str) -> XbrlResult {
    let mut errors = Vec::new();
    let mut facts = Vec::with_capacity(request.concepts.len());

    for concept in &request.concepts {
        facts.push(vimbai_reporting::xbrl::fact(
            &request.taxonomy,
            &concept.concept,
            concept.value,
            json!(concept.context_ref),
            json!(concept.unit_ref),
            json!(concept.decimals),
        ));
    }

    errors.extend(vimbai_reporting::xbrl::missing_concepts(
        request
            .concepts
            .iter()
            .map(|concept| concept.concept.as_str()),
    ));

    XbrlResult {
        company_id: request.company_id.clone(),
        entity_name: request.entity_name.clone(),
        fiscal_year: request.fiscal_year,
        taxonomy: request.taxonomy.clone(),
        concept_count: request.concepts.len(),
        validation_status: if errors.is_empty() {
            "valid"
        } else {
            "invalid"
        }
        .to_string(),
        validation_errors: errors,
        xbrl_facts: facts,
        filing_metadata: json!({
            "entity_identifier": request.entity_identifier,
            "reporting_period": format!("FY{}", request.fiscal_year),
            "generated_at": generated_at,
            "schema": format!("https://xbrl.ifrs.org/taxonomy/{}/", request.taxonomy),
            "language": "en",
        }),
    }
}
