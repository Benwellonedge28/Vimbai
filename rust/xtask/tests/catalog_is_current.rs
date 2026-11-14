#![forbid(unsafe_code)]
//! The catalogue has to agree with the gateway and the ownership table, or it is a third opinion.
//!
//! These assertions are the ones the Python test made before the generator moved here: every routed service appears
//! exactly once, every service has an owner, the port and path are the gateway's, and the committed files match the
//! generator. What changed is that the same functions now write the files and check them, so a stale catalogue fails
//! here instead of being discovered by whoever reads it next.

use xtask::catalog::{catalog_text, domain_texts, readme_text, repo_root, rows};

#[test]
fn every_routed_service_appears_exactly_once() {
    let root = repo_root();
    let listed: Vec<String> = rows(&root).into_iter().map(|row| row.name).collect();
    let mut unique = listed.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        listed.len(),
        "a service is listed twice: {listed:?}"
    );
}

#[test]
fn every_service_has_an_owner_domain() {
    let root = repo_root();
    let unowned: Vec<String> = rows(&root)
        .into_iter()
        .filter(|row| row.domain.is_empty())
        .map(|row| row.name)
        .collect();
    assert!(unowned.is_empty(), "routed but unowned: {unowned:?}");
}

#[test]
fn ports_and_paths_are_the_gateways_own() {
    let root = repo_root();
    let text = std::fs::read_to_string(root.join("gateway/go/config/services.json")).unwrap();
    let registry: serde_json::Value = serde_json::from_str(&text).unwrap();
    let services = registry["services"].as_array().unwrap();
    for row in rows(&root) {
        let entry = services
            .iter()
            .find(|entry| entry["name"].as_str() == Some(row.name.as_str()))
            .unwrap_or_else(|| panic!("{} is in the catalogue but not in the gateway", row.name));
        assert_eq!(entry["port"].as_i64(), row.port, "{}", row.name);
        assert_eq!(
            entry["path"].as_str().unwrap_or_default(),
            row.gateway_path,
            "{}",
            row.name
        );
    }
}

#[test]
fn the_committed_catalogue_matches_the_generator() {
    let root = repo_root();
    let committed = std::fs::read_to_string(root.join("catalog/services/catalog.yaml")).unwrap();
    assert_eq!(
        committed,
        catalog_text(&root),
        "catalog/services/catalog.yaml is stale, run `cargo run -p xtask -- catalog`"
    );
}

#[test]
fn each_domain_file_matches_the_generator() {
    let root = repo_root();
    let written = domain_texts(&root);
    let directory = root.join("catalog/services");
    let mut on_disk: Vec<String> = std::fs::read_dir(&directory)
        .unwrap()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            (name.ends_with(".yaml") && name != "catalog.yaml")
                .then(|| name.trim_end_matches(".yaml").to_string())
        })
        .collect();
    on_disk.sort();
    let mut expected: Vec<String> = written.iter().map(|(domain, _)| domain.clone()).collect();
    expected.sort();
    assert_eq!(on_disk, expected, "a domain file is missing or stale");

    for (domain, text) in written {
        let committed = std::fs::read_to_string(directory.join(format!("{domain}.yaml"))).unwrap();
        assert_eq!(committed, text, "{domain}.yaml is stale");
    }
}

#[test]
fn the_readme_describes_the_files_that_exist() {
    let root = repo_root();
    let committed = std::fs::read_to_string(root.join("catalog/README.md")).unwrap();
    assert_eq!(committed, readme_text(), "catalog/README.md is stale");
}

#[test]
fn no_service_is_reported_as_python() {
    // Every routed service is served by the Rust engine. The payment gateway, which used to be the one exception, was
    // deleted from the repository: money movement is outside Vimbai's remit, so there is nothing left for it to do.
    // If the catalogue names a Python service now, a port regressed or the port tracker stopped being read.
    let root = repo_root();
    let python: Vec<String> = rows(&root)
        .into_iter()
        .filter(|row| row.engine == "python")
        .map(|row| row.name)
        .collect();
    assert!(
        python.is_empty(),
        "the catalogue reports Python services: {python:?}"
    );
}
