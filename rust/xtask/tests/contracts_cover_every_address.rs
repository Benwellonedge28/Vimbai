#![forbid(unsafe_code)]
//! Both directions, over every address: a contract nobody serves and a route nobody documented are the same defect.

use xtask::catalog::repo_root;
use xtask::openapi;

#[test]
fn every_published_address_has_a_contract_and_every_contract_is_answered() {
    let findings = openapi::problems(&repo_root());
    assert!(
        findings.is_empty(),
        "{} finding(s):\n{}",
        findings.len(),
        findings.join("\n")
    );
}

#[test]
fn the_inventory_is_large_enough_for_the_check_to_mean_something() {
    // a coverage gate over an empty set passes over nothing, which has bitten this repository before
    let root = repo_root();
    let routes = xtask::gateway::engine_method_routes(&root);
    assert!(
        routes.len() > 1000,
        "only {} routes were found: the scanner is probably not reading the api-server",
        routes.len()
    );
    let generated = openapi::expected(&root);
    assert!(
        generated.len() >= 20,
        "only {} specifications would be generated",
        generated.len()
    );
    let documented: usize = generated
        .values()
        .map(|text| openapi::documented_paths(text).len())
        .sum();
    assert!(
        documented > 1000,
        "the generated specifications cover only {documented} paths"
    );
}

#[test]
fn the_committed_specifications_are_what_the_generator_writes() {
    let findings = openapi::stale(&repo_root());
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}

#[test]
fn a_path_is_documented_exactly_once() {
    // the same address promised by two documents is two promises that can disagree
    let root = repo_root();
    let mut seen: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut doubled = Vec::new();
    for directory in [openapi::SERVICE_DIR, openapi::DOMAIN_DIR] {
        let path = root.join(directory);
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".yaml") {
                continue;
            }
            let text = std::fs::read_to_string(entry.path()).expect("a contract");
            for documented in openapi::documented_paths(&text) {
                if let Some(first) = seen.insert(documented.clone(), name.clone()) {
                    doubled.push(format!("{documented} in both {first} and {name}"));
                }
            }
        }
    }
    assert!(doubled.is_empty(), "{}", doubled.join("\n"));
}

#[test]
fn the_hand_written_contracts_kept_their_detail() {
    // the six were written for a reason: a schema and prose a generator cannot produce. Moving them must not thin them.
    let root = repo_root();
    let directory = root.join(openapi::SERVICE_DIR);
    for name in [
        "accounting.yaml",
        "assets.yaml",
        "costing.yaml",
        "equity.yaml",
        "job-costing.yaml",
        "treasury.yaml",
    ] {
        let text =
            std::fs::read_to_string(directory.join(name)).unwrap_or_else(|_| panic!("{name}"));
        assert!(
            text.contains("components:") && text.contains("schemas:"),
            "{name} lost its schemas"
        );
        assert!(
            text.lines().count() > 100,
            "{name} is only {} lines",
            text.lines().count()
        );
    }
}
