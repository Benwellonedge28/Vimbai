#![forbid(unsafe_code)]
//! accounting-standards-service, on the store.
//!
//! The standards catalogue itself is code-defined data, not records: fourteen standards and their
//! requirements, lifted unchanged from the service's own definitions into
//! `assets/accounting-standards.json` so nothing in the catalogue is retyped by hand. What a caller
//! owns lives on the store and is Book-scoped: an organization's standard configuration, its account
//! mappings, its accounting policies, and its compliance check history.
//!
//! Nothing here moves money, so there are no amounts to keep exact.
//!
//! The shapes, as shipped, each pinned by a test:
//!
//! * configuring an organization replaces its configuration rather than adding a second one, and a
//!   policy is replaced per organization, standard and policy area;
//! * activating a standard the organization already has changes nothing, and deactivating one it does
//!   not have changes nothing either; both answer as if they had;
//! * a standard's requirements are answered as an empty list for a standard the catalogue does not
//!   describe, while asking for the standard itself refuses;
//! * a compliance check writes one check per requirement of the standard, passing unless the caller's
//!   validation results say otherwise for that requirement code, and replaces the organization's
//!   earlier history rather than appending to it;
//! * the counts in a compliance answer are taken from the checks just written, so a status of anything
//!   other than pass, fail or warning is counted in none of them.

use std::sync::OnceLock;

use serde_json::{json, Value};
use vimbai_store::{
    created_record, BookScope, Graph, Records, StoreError, ACCOUNTING_POLICY, ACCOUNT_MAPPING,
    COMPLIANCE_CHECK, STANDARDS_CONFIG,
};

/// The catalogue as the service defines it: standards and their requirements.
const CATALOGUE: &str = include_str!("../assets/accounting-standards.json");

fn catalogue() -> &'static Value {
    static PARSED: OnceLock<Value> = OnceLock::new();
    PARSED.get_or_init(|| serde_json::from_str(CATALOGUE).expect("the standards catalogue parses"))
}

fn standards() -> &'static Vec<Value> {
    static STANDARDS: OnceLock<Vec<Value>> = OnceLock::new();
    STANDARDS.get_or_init(|| {
        catalogue()
            .get("standards")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    })
}

fn requirements() -> &'static Value {
    static REQUIRED: OnceLock<Value> = OnceLock::new();
    REQUIRED.get_or_init(|| {
        catalogue()
            .get("requirements")
            .cloned()
            .unwrap_or_else(|| json!({}))
    })
}

/// The catalogue's own key for a standard: the value its type is spelled with.
fn standard_of(value: &str) -> Option<&'static Value> {
    standards()
        .iter()
        .find(|standard| standard.get("standard_type").and_then(Value::as_str) == Some(value))
}

fn standard_matching(value: &str) -> Option<&'static Value> {
    standard_of(value).or_else(|| {
        standards()
            .iter()
            .find(|standard| standard.get("key").and_then(Value::as_str) == Some(value))
    })
}

/// A lookup the catalogue, the store or the caller's own records do not satisfy.
fn refused(_message: &str) -> StoreError {
    StoreError::NotFound
}

fn text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn optional(payload: &Value, key: &str) -> Option<String> {
    let value = text(payload, key);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn or_default(payload: &Value, key: &str, fallback: &str) -> String {
    optional(payload, key).unwrap_or_else(|| fallback.to_string())
}

fn strings(payload: &Value, key: &str) -> Vec<String> {
    match payload.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::String(text)) => text
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn field<'a>(record: &'a Value, key: &str) -> &'a str {
    record.get(key).and_then(Value::as_str).unwrap_or("")
}

fn written(body: Value, saved: &Value) -> Value {
    let mut answer = created_record(saved);
    match (answer.as_object_mut(), body.as_object()) {
        (Some(answer), Some(fields)) => {
            for (key, value) in fields {
                answer.entry(key.clone()).or_insert_with(|| value.clone());
            }
            Value::Object(answer.clone())
        }
        _ => body,
    }
}

fn requirement_rows(standard: &str) -> Vec<Value> {
    requirements()
        .get(standard)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

// --- the catalogue ---

/// `/standards`, optionally narrowed by region or type.
pub fn list_standards(query: &Value) -> Value {
    let region = optional(query, "region").map(|value| value.to_lowercase());
    let kind = optional(query, "standard_type");
    let rows: Vec<Value> = standards()
        .iter()
        .filter(|standard| {
            region
                .as_deref()
                .map(|want| field(standard, "region").to_lowercase() == want)
                .unwrap_or(true)
                && kind
                    .as_deref()
                    .map(|want| field(standard, "standard_type") == want)
                    .unwrap_or(true)
        })
        .cloned()
        .collect();
    Value::Array(rows)
}

/// `/standards/{type}`.
pub fn get_standard(standard_type: &str) -> Result<Value, StoreError> {
    standard_matching(standard_type)
        .cloned()
        .ok_or_else(|| refused("Standard not found"))
}

/// `/standards/comparison`.
pub fn compare_standards(first: &str, second: &str) -> Result<Value, StoreError> {
    let one = get_standard(first)?;
    let two = get_standard(second)?;
    let principles = |standard: &Value| -> Vec<String> {
        standard
            .get("key_principles")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let one_principles = principles(&one);
    let two_principles = principles(&two);

    Ok(json!({
        "standard_1": one,
        "standard_2": two,
        "comparison": {
            "measurement_basis_differences":
                field(&one, "measurement_basis") != field(&two, "measurement_basis"),
            "consolidation_method_differences":
                field(&one, "consolidation_method") != field(&two, "consolidation_method"),
            "key_principles_common": one_principles
                .iter()
                .filter(|principle| two_principles.contains(principle))
                .cloned()
                .collect::<Vec<_>>(),
            "unique_to_standard_1": one_principles
                .iter()
                .filter(|principle| !two_principles.contains(principle))
                .cloned()
                .collect::<Vec<_>>(),
            "unique_to_standard_2": two_principles
                .iter()
                .filter(|principle| !one_principles.contains(principle))
                .cloned()
                .collect::<Vec<_>>(),
        },
    }))
}

/// `/standards/categories`: the standards grouped by region.
pub fn standard_categories() -> Value {
    let mut categories = serde_json::Map::new();
    for standard in standards() {
        let region = field(standard, "region").to_string();
        let entry = categories
            .entry(region)
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(items) = entry.as_array_mut() {
            items.push(json!({
                "code": field(standard, "standard_type"),
                "name": field(standard, "name"),
                "country": field(standard, "country"),
            }));
        }
    }
    Value::Object(categories)
}

/// `/standards/{type}/requirements`.
pub fn standard_requirements(standard_type: &str) -> Value {
    Value::Array(requirement_rows(standard_type))
}

/// `/standards/{type}/disclosure-requirements`.
pub fn disclosure_requirements(standard_type: &str) -> Value {
    let rows: Vec<Value> = requirement_rows(standard_type)
        .into_iter()
        .filter(|requirement| requirement.get("disclosure_required") == Some(&json!(true)))
        .map(|requirement| {
            json!({
                "code": field(&requirement, "requirement_code"),
                "description": field(&requirement, "description"),
                "category": field(&requirement, "category"),
                "is_mandatory": requirement.get("is_mandatory").cloned().unwrap_or(Value::Null),
                "disclosure_required": true,
            })
        })
        .collect();
    Value::Array(rows)
}

/// `/standards/{type}/measurement-guide`.
pub fn measurement_guide(standard_type: &str) -> Result<Value, StoreError> {
    let standard = get_standard(standard_type)?;
    let basis = field(&standard, "measurement_basis").to_string();
    let guidance = match field(&standard, "standard_type") {
        "ifrs" => json!({
            "fair_value": "Use market participant assumptions, prioritize observable inputs",
            "historical_cost": "Rarely used except for some assets under IFRS 9",
            "present_value": "Discount using market rate for similar instruments",
        }),
        "us_gaap" => json!({
            "historical_cost": "Primary measurement basis, fair value option available",
            "fair_value": "Used for financial instruments, asset impairments, business combinations",
            "present_value": "Used for leases, asset retirement obligations, environmental liabilities",
        }),
        _ => json!({"default": format!("Measurement basis: {basis}")}),
    };
    Ok(json!({
        "standard": field(&standard, "standard_type"),
        "measurement_basis": basis,
        "guidance": guidance,
    }))
}

// --- an organization's configuration ---

fn config_body(
    organization_id: &str,
    payload: &Value,
    created_at: &str,
    updated_at: &str,
) -> Value {
    json!({
        "organization_id": organization_id,
        "primary_standard": or_default(payload, "primary_standard", "ifrs"),
        "selected_standards": strings(payload, "selected_standards"),
        "fiscal_year_end": text(payload, "fiscal_year_end"),
        "reporting_currency": or_default(payload, "reporting_currency", "USD"),
        "consolidation_required": payload
            .get("consolidation_required")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "created_at": created_at,
        "updated_at": updated_at,
    })
}

fn configuration_of<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(Records::new(graph, STANDARDS_CONFIG)
        .list(scope, 1000)?
        .into_iter()
        .find(|row| field(row, "organization_id") == organization_id))
}

fn replace_configuration<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, STANDARDS_CONFIG);
    let existing = configuration_of(graph, scope, organization_id)?;
    let created_at = existing
        .as_ref()
        .map(|row| field(row, "created_at").to_string())
        .unwrap_or_else(|| text(payload, "created_at"));
    let body = config_body(
        organization_id,
        payload,
        &created_at,
        &text(payload, "updated_at"),
    );
    if let Some(existing) = existing {
        records.delete(scope, field(&existing, "id"))?;
    }
    Ok(written(body.clone(), &records.create(scope, &body)?))
}

/// `POST /organizations/{id}/configuration`: replaces the caller's configuration.
pub fn configure_organization<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    replace_configuration(graph, scope, organization_id, payload)
}

/// `GET /organizations/{id}/configuration`.
pub fn get_configuration<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
) -> Result<Value, StoreError> {
    configuration_of(graph, scope, organization_id)?
        .ok_or_else(|| refused("Configuration not found"))
}

/// `PUT /organizations/{id}/configuration`, which needs one to exist first.
pub fn update_configuration<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    if configuration_of(graph, scope, organization_id)?.is_none() {
        return Err(refused("Configuration not found"));
    }
    replace_configuration(graph, scope, organization_id, payload)
}

fn toggle_standard<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
    standard_type: &str,
    activate: bool,
    stamp: &str,
) -> Result<Value, StoreError> {
    let existing = configuration_of(graph, scope, organization_id)?
        .ok_or_else(|| refused("Organization configuration not found"))?;
    let mut selected = strings(&existing, "selected_standards");
    let present = selected.iter().any(|value| value == standard_type);
    // a standard already active, or already absent, is left exactly as it is
    if activate && !present {
        selected.push(standard_type.to_string());
    } else if !activate && present {
        selected.retain(|value| value != standard_type);
    } else {
        return Ok(json!({
            "status": if activate { "activated" } else { "deactivated" },
            "standard": standard_type,
        }));
    }

    let mut body = existing.clone();
    body["selected_standards"] = json!(selected);
    body["updated_at"] = json!(stamp);
    let records = Records::new(graph, STANDARDS_CONFIG);
    records.delete(scope, field(&existing, "id"))?;
    records.create(scope, &body)?;

    Ok(json!({
        "status": if activate { "activated" } else { "deactivated" },
        "standard": standard_type,
    }))
}

/// `POST /organizations/{id}/standards/{type}/activate`.
pub fn activate_standard<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
    standard_type: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    toggle_standard(
        graph,
        scope,
        organization_id,
        standard_type,
        true,
        &text(payload, "updated_at"),
    )
}

/// `POST /organizations/{id}/standards/{type}/deactivate`.
pub fn deactivate_standard<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
    standard_type: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    toggle_standard(
        graph,
        scope,
        organization_id,
        standard_type,
        false,
        &text(payload, "updated_at"),
    )
}

// --- account mappings ---

/// `/standards/{type}/account-mapping` listing.
pub fn account_mappings<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    standard_type: &str,
) -> Result<Value, StoreError> {
    let rows: Vec<Value> = Records::new(graph, ACCOUNT_MAPPING)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| field(row, "standard_type") == standard_type)
        .collect();
    Ok(Value::Array(rows))
}

/// `POST /standards/{type}/account-mapping`: the path decides the standard.
pub fn add_account_mapping<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    standard_type: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let body = json!({
        "standard_type": standard_type,
        "account_code": text(payload, "account_code"),
        "account_name": text(payload, "account_name"),
        "account_category": or_default(payload, "account_category", "asset"),
        "mapped_to": text(payload, "mapped_to"),
        "notes": text(payload, "notes"),
        "created_at": text(payload, "created_at"),
    });
    let mapping = written(
        body.clone(),
        &Records::new(graph, ACCOUNT_MAPPING).create(scope, &body)?,
    );
    Ok(json!({"status": "added", "mapping": mapping}))
}

// --- policies ---

/// `POST /organizations/{id}/policies`: replaced per organization, standard and area.
pub fn create_policy<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let standard_type = or_default(payload, "standard_type", "ifrs");
    let policy_area = text(payload, "policy_area");
    let body = json!({
        "organization_id": organization_id,
        "standard_type": standard_type,
        "policy_area": policy_area,
        "policy_title": text(payload, "policy_title"),
        "policy_description": text(payload, "policy_description"),
        "measurement_base": text(payload, "measurement_base"),
        "disclosure_level": text(payload, "disclosure_level"),
        "custom_guidance": text(payload, "custom_guidance"),
        "approved_by": text(payload, "approved_by"),
        "effective_date": text(payload, "effective_date"),
        "created_at": text(payload, "created_at"),
    });

    let records = Records::new(graph, ACCOUNTING_POLICY);
    for row in records.list(scope, 1000)? {
        let same = field(&row, "organization_id") == organization_id
            && field(&row, "standard_type") == standard_type
            && field(&row, "policy_area") == policy_area;
        if same {
            records.delete(scope, field(&row, "id"))?;
        }
    }
    Ok(written(body.clone(), &records.create(scope, &body)?))
}

/// `/organizations/{id}/policies` listing.
pub fn list_policies<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let standard = optional(query, "standard_type");
    let rows: Vec<Value> = Records::new(graph, ACCOUNTING_POLICY)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| {
            field(row, "organization_id") == organization_id
                && standard
                    .as_deref()
                    .map(|want| field(row, "standard_type") == want)
                    .unwrap_or(true)
        })
        .collect();
    Ok(Value::Array(rows))
}

/// `/organizations/{id}/policies/{area}`: one area for one standard.
pub fn policy_for_area<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
    policy_area: &str,
    query: &Value,
) -> Result<Value, StoreError> {
    let standard = text(query, "standard_type");
    Records::new(graph, ACCOUNTING_POLICY)
        .list(scope, 1000)?
        .into_iter()
        .find(|row| {
            field(row, "organization_id") == organization_id
                && field(row, "policy_area") == policy_area
                && field(row, "standard_type") == standard
        })
        .ok_or_else(|| refused("Policy not found"))
}

// --- compliance ---

/// `POST /organizations/{id}/compliance/check`: one check per requirement, replacing the history.
pub fn run_compliance_check<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let standard_type = or_default(payload, "standard_type", "ifrs");
    let validations = payload
        .get("validation_results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let check_date = text(payload, "check_date");

    let mut checks = Vec::new();
    for requirement in requirement_rows(&standard_type) {
        let code = field(&requirement, "requirement_code").to_string();
        let mut check = json!({
            "organization_id": organization_id,
            "standard_type": standard_type,
            "check_date": check_date,
            "status": "pass",
            "area": field(&requirement, "category"),
            "requirement": code,
            "finding": Value::Null,
            "severity": Value::Null,
            "recommendation": Value::Null,
        });
        if let Some(result) = validations
            .iter()
            .find(|result| field(result, "code") == code)
        {
            check["status"] = json!(or_default(result, "status", "pass"));
            check["finding"] = result.get("finding").cloned().unwrap_or(Value::Null);
            check["severity"] = result.get("severity").cloned().unwrap_or(Value::Null);
            check["recommendation"] = result.get("recommendation").cloned().unwrap_or(Value::Null);
        }
        checks.push(check);
    }

    let records = Records::new(graph, COMPLIANCE_CHECK);
    for row in records.list(scope, 1000)? {
        if field(&row, "organization_id") == organization_id {
            records.delete(scope, field(&row, "id"))?;
        }
    }
    let mut stored = Vec::new();
    for check in &checks {
        records.create(scope, check)?;
        stored.push(check.clone());
    }

    let count = |status: &str| {
        stored
            .iter()
            .filter(|check| field(check, "status") == status)
            .count()
    };

    Ok(json!({
        "organization_id": organization_id,
        "standard": standard_type,
        "total_checks": stored.len(),
        "passed": count("pass"),
        "failed": count("fail"),
        "warnings": count("warning"),
        "checks": stored,
    }))
}

/// `/organizations/{id}/compliance/history`.
pub fn compliance_history<G: Graph + ?Sized>(
    graph: &G,
    scope: &BookScope,
    organization_id: &str,
) -> Result<Value, StoreError> {
    let rows: Vec<Value> = Records::new(graph, COMPLIANCE_CHECK)
        .list(scope, 1000)?
        .into_iter()
        .filter(|row| field(row, "organization_id") == organization_id)
        .collect();
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::FakeGraph;

    fn scope() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    #[test]
    fn the_catalogue_keeps_every_standard_it_shipped() {
        assert_eq!(standards().len(), 14);
        // the keys the service answered by, and the type it files each under
        let ifrs = standard_matching("ifrs").expect("ifrs");
        assert_eq!(field(ifrs, "code"), "IFRS");
        assert_eq!(field(ifrs, "region"), "International");
        assert_eq!(field(ifrs, "measurement_basis"), "fair_value");
        // the UK standard is keyed by its type, but its code is the standard's own name
        let uk = standard_matching("uk_gaap").expect("uk");
        assert_eq!(field(uk, "code"), "FRS 102");
    }

    #[test]
    fn standards_are_listed_and_narrowed() {
        let all = list_standards(&json!({}));
        assert_eq!(all.as_array().expect("standards").len(), 14);
        let asia = list_standards(&json!({"region": "asia"}));
        assert!(asia.as_array().expect("asia").len() >= 3, "{asia}");
        let uk = list_standards(&json!({"standard_type": "uk_gaap"}));
        assert_eq!(uk.as_array().expect("uk").len(), 1);
    }

    #[test]
    fn an_unknown_standard_is_refused_but_its_requirements_are_empty() {
        assert!(get_standard("mars_gaap").is_err());
        assert_eq!(standard_requirements("mars_gaap"), json!([]));
        assert_eq!(disclosure_requirements("mars_gaap"), json!([]));
    }

    #[test]
    fn the_comparison_finds_what_only_one_standard_holds() {
        let answer = compare_standards("ifrs", "us_gaap").expect("compared");
        assert_eq!(
            answer["comparison"]["measurement_basis_differences"],
            json!(true)
        );
        let shared = answer["comparison"]["key_principles_common"]
            .as_array()
            .expect("common");
        assert!(shared.is_empty(), "the two share no wording: {shared:?}");
    }

    #[test]
    fn the_requirements_and_disclosures_come_from_the_catalogue() {
        let all = standard_requirements("ifrs");
        assert_eq!(all.as_array().expect("requirements").len(), 4);
        let disclosed = disclosure_requirements("ifrs");
        assert_eq!(disclosed.as_array().expect("disclosures").len(), 4);
        assert_eq!(disclosed[0]["code"], json!("IFRS_15"));
    }

    #[test]
    fn the_measurement_guide_follows_the_standard() {
        let ifrs = measurement_guide("ifrs").expect("guide");
        assert_eq!(ifrs["measurement_basis"], json!("fair_value"));
        assert!(ifrs["guidance"]["fair_value"].is_string());
        let other = measurement_guide("south_african_gaap").expect("guide");
        assert!(other["guidance"]["default"]
            .as_str()
            .expect("default")
            .starts_with("Measurement basis:"));
    }

    #[test]
    fn configuring_an_organization_replaces_its_configuration() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "c-9", "organization_id": "org-1", "created_at": "2026-01-01"})],
            vec![json!({"id": "c-9"})],
            vec![json!({"id": "c-1"})],
        ]);
        let answer = configure_organization(
            &graph,
            &scope(),
            "org-1",
            &json!({"primary_standard": "ifrs", "reporting_currency": "USD",
                    "selected_standards": ["ifrs"]}),
        )
        .expect("configured");
        assert_eq!(answer["id"], json!("c-1"));
        assert_eq!(answer["organization_id"], json!("org-1"));
        // the earlier configuration was removed rather than left beside the new one
        let (cypher, _) = graph.statement(1);
        assert!(cypher.contains("DETACH DELETE"), "{cypher}");
    }

    #[test]
    fn activating_a_standard_the_organization_already_has_changes_nothing() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "c-1", "organization_id": "org-1", "selected_standards": ["ifrs"]
        })]);
        let answer =
            activate_standard(&graph, &scope(), "org-1", "ifrs", &json!({})).expect("activated");
        assert_eq!(answer["status"], json!("activated"));
        // nothing was written: the only statement was the read
        assert_eq!(graph.statements(), 1);
    }

    #[test]
    fn activating_a_new_standard_stores_it_beside_the_others() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "c-1", "organization_id": "org-1",
                        "selected_standards": ["ifrs"]})],
            vec![json!({"id": "c-1"})],
            vec![json!({"id": "c-2"})],
        ]);
        activate_standard(
            &graph,
            &scope(),
            "org-1",
            "us_gaap",
            &json!({"updated_at": "now"}),
        )
        .expect("activated");
        let (_, params) = graph.statement(2);
        assert_eq!(
            params["props"]["selected_standards"],
            json!(["ifrs", "us_gaap"])
        );
    }

    #[test]
    fn a_compliance_check_passes_every_requirement_until_told_otherwise() {
        let graph = FakeGraph::queue(vec![
            vec![],
            vec![json!({"id": "k-1"})],
            vec![json!({"id": "k-2"})],
            vec![json!({"id": "k-3"})],
            vec![json!({"id": "k-4"})],
        ]);
        let answer = run_compliance_check(
            &graph,
            &scope(),
            "org-1",
            &json!({"standard_type": "ifrs",
            "validation_results": [
                {"code": "IFRS_16", "status": "fail", "severity": "high"},
                {"code": "IFRS_13", "status": "warning"}
            ]}),
        )
        .expect("checked");
        assert_eq!(answer["total_checks"], json!(4));
        assert_eq!(answer["passed"], json!(2));
        assert_eq!(answer["failed"], json!(1));
        assert_eq!(answer["warnings"], json!(1));
        let failed = answer["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .find(|check| check["requirement"] == json!("IFRS_16"))
            .cloned()
            .expect("ifrs 16");
        assert_eq!(failed["status"], json!("fail"));
        assert_eq!(failed["area"], json!("Leases"));
    }

    #[test]
    fn a_second_check_replaces_the_first_history() {
        let graph = FakeGraph::queue(vec![
            vec![json!({"id": "k-9", "organization_id": "org-1"})],
            vec![json!({"id": "k-9"})],
            vec![json!({"id": "k-1"})],
            vec![json!({"id": "k-2"})],
            vec![json!({"id": "k-3"})],
            vec![json!({"id": "k-4"})],
        ]);
        run_compliance_check(&graph, &scope(), "org-1", &json!({"standard_type": "ifrs"}))
            .expect("checked");
        let (cypher, _) = graph.statement(1);
        assert!(cypher.contains("DETACH DELETE"), "{cypher}");
    }

    #[test]
    fn a_standard_with_no_requirements_records_no_checks() {
        let graph = FakeGraph::queue(vec![
            vec![],
            vec![json!({"id": "k-1"})],
            vec![json!({"id": "k-2"})],
        ]);
        let answer = run_compliance_check(
            &graph,
            &scope(),
            "org-1",
            &json!({"standard_type": "us_gaap"}),
        )
        .expect("checked");
        assert_eq!(answer["total_checks"], json!(2));
        assert_eq!(answer["passed"], json!(2));
    }

    #[test]
    fn a_policy_is_replaced_per_area_and_standard() {
        let graph = FakeGraph::queue(vec![
            vec![
                json!({"id": "p-9", "organization_id": "org-1", "standard_type": "ifrs",
                       "policy_area": "revenue"}),
                json!({"id": "p-8", "organization_id": "org-1", "standard_type": "ifrs",
                       "policy_area": "leases"}),
            ],
            vec![json!({"id": "p-9"})],
            vec![json!({"id": "p-1"})],
        ]);
        let answer = create_policy(
            &graph,
            &scope(),
            "org-1",
            &json!({"standard_type": "ifrs", "policy_area": "revenue",
                    "policy_title": "Five step model"}),
        )
        .expect("policy");
        assert_eq!(answer["policy_area"], json!("revenue"));
        // only the revenue policy was removed; the leases one stands
        let (cypher, body) = graph.statement(1);
        assert!(cypher.contains("DETACH DELETE"), "{cypher}");
        let id = body.get("id").and_then(Value::as_str).unwrap_or_default();
        assert_eq!(id, "p-9");
    }

    #[test]
    fn an_account_mapping_takes_its_standard_from_the_path() {
        let graph = FakeGraph::returning(vec![json!({"id": "m-1"})]);
        let answer = add_account_mapping(
            &graph,
            &scope(),
            "us_gaap",
            &json!({"account_code": "4000", "account_name": "Revenue",
                    "account_category": "revenue"}),
        )
        .expect("mapping");
        assert_eq!(answer["status"], json!("added"));
        assert_eq!(answer["mapping"]["standard_type"], json!("us_gaap"));
        let (_, params) = graph.statement(0);
        assert_eq!(params["props"]["standard_type"], json!("us_gaap"));
    }

    #[test]
    fn a_policy_for_an_area_that_was_never_written_is_refused() {
        let graph = FakeGraph::returning(vec![json!({
            "id": "p-1", "organization_id": "org-1", "standard_type": "ifrs",
            "policy_area": "leases"
        })]);
        assert!(policy_for_area(
            &graph,
            &scope(),
            "org-1",
            "revenue",
            &json!({"standard_type": "ifrs"})
        )
        .is_err());
    }
}
