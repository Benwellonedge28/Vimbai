//! Parity: the figures the retired Python services answered, still answered by the engine.
//!
//! Each fixture under `rust/tests/fixtures/parity/` was captured from a service before its directory was deleted. This
//! test reads them and calls the Rust calculator the retired address now reaches. It is the answer to "was the port a
//! port, or a rewrite?": the numbers the old service produced come back unchanged.
//!
//! A fixture is evidence, so this test never writes one. If a figure changes, it fails.

use serde_json::{json, Value};
use std::path::PathBuf;
use vimbai_service_costing::absorption;
use vimbai_service_treasury::treasury_analytics;
use vimbai_store::fake::MemoryGraph;
use vimbai_store::BookScope;

fn parity_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/parity")
}

/// Every fixture, so a new one cannot be added without this test noticing it exists.
fn fixtures() -> Vec<(String, Value)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(parity_dir()).expect("the parity fixtures") {
        let path = entry.expect("an entry").path();
        if path.extension().map(|kind| kind != "json").unwrap_or(true) {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("a fixture");
        let parsed: Value = serde_json::from_str(&text).expect("a JSON fixture");
        found.push((
            path.file_stem()
                .expect("a name")
                .to_string_lossy()
                .to_string(),
            parsed,
        ));
    }
    found.sort_by(|left, right| left.0.cmp(&right.0));
    assert!(
        !found.is_empty(),
        "the parity fixtures are the point of this file"
    );
    found
}

fn cases<'a>(fixture: &'a Value, service: &str) -> &'a Vec<Value> {
    assert_eq!(
        fixture["service"],
        json!(service),
        "a fixture must name the service it came from"
    );
    fixture["cases"].as_array().expect("cases")
}

fn assert_figures(answer: &Value, expected: &Value, case: &Value) {
    let expected = expected.as_object().expect("expected figures");
    for (key, want) in expected {
        assert_eq!(
            answer.get(key),
            Some(want),
            "case {:?}: {key} must be {want}, the figure the Python service answered\nwhole answer: {answer}",
            case["name"]
        );
    }
}

#[test]
fn the_treasury_analytics_service_figures_come_back_unchanged() {
    for (_, fixture) in fixtures() {
        if fixture["service"] != json!("treasury-analytics-service") {
            continue;
        }
        for case in cases(&fixture, "treasury-analytics-service") {
            let graph = MemoryGraph::default();
            let scope = BookScope::new("user-1", "book-1");
            let answer = treasury_analytics::analyze(&graph, &scope, &case["payload"])
                .expect("the published analysis answers");
            assert_figures(&answer, &case["expected"], case);
        }
    }
}

#[test]
fn the_absorption_costing_service_figures_come_back_unchanged() {
    for (_, fixture) in fixtures() {
        if fixture["service"] != json!("absorption-costing-service") {
            continue;
        }
        for case in cases(&fixture, "absorption-costing-service") {
            let address = case["address"].as_str().expect("the address it published");
            let answer = match address {
                "/product-costs/calculate" => {
                    absorption::calculate_product_cost(&case["payload"]).expect("a product cost")
                }
                "/overhead/absorption" => {
                    absorption::absorb_overhead(&case["payload"]).expect("an absorption")
                }
                "/cost-plus" => {
                    absorption::cost_plus_selling_price(&case["payload"]).expect("a selling price")
                }
                other => panic!("a fixture names an address nothing answers: {other}"),
            };
            assert_figures(&answer, &case["expected"], case);
        }
    }
}

#[test]
fn every_fixture_names_a_service_this_file_checks() {
    // a fixture nothing asserts is a claim nobody reads: adding one means adding its check
    let mut unchecked = Vec::new();
    for (name, fixture) in fixtures() {
        let service = fixture["service"].as_str().unwrap_or_default();
        if !matches!(
            service,
            "treasury-analytics-service" | "absorption-costing-service"
        ) {
            unchecked.push(format!("{name} -> {service}"));
        }
    }
    assert!(
        unchecked.is_empty(),
        "fixtures with no assertion here: {}",
        unchecked.join(", ")
    );
}
