#![forbid(unsafe_code)]
//! The repository's own rules, held over the repository rather than over a service.
//!
//! Each rule here replaced a Python test that read the same files. They are gathered in one file for a reason: what they
//! check is the shape of the estate - where code lives, what is registered once, what the cluster deploys, what the
//! contracts promise - and a reader looking for "how is this repository kept honest" should find it in one place.

use std::path::PathBuf;

use xtask::{boundaries, catalog, crates, manifests, reachability, wiring, workflows};

fn root() -> PathBuf {
    xtask::catalog::repo_root()
}

#[test]
fn the_store_holds_no_service_module() {
    // A service belongs in its domain crate. The store knows the record statements and nothing about what a record
    // means, and this is the rule that was broken first and quietly: fourteen modules side by side in the adapter.
    let extra = crates::store_service_modules(&root());
    assert!(
        extra.is_empty(),
        "the store grew a service module again: {extra:?}"
    );
}

#[test]
fn the_store_declares_no_domain_module() {
    let declared = crates::store_declared_domains(&root());
    assert!(
        declared.is_empty(),
        "the store still re-exports a domain module: {declared:?}"
    );
}

#[test]
fn every_rust_service_has_a_crate_and_is_named_by_it() {
    let missing = crates::services_without_a_crate(&root());
    assert!(
        missing.is_empty(),
        "a Rust service has no crate: {missing:?}"
    );
    let unnamed = crates::services_unnamed_by_their_crate(&root());
    assert!(unnamed.is_empty(), "{unnamed:?}");
}

#[test]
fn every_crate_is_safe_rust_only() {
    // Swept across the whole workspace, not the domains alone: the kernel crate carried no attribute at all while the
    // sweep looked elsewhere, which is the one crate where an unsafe block would matter most.
    let gaps = crates::crates_without_the_safety_attribute(&root());
    assert!(
        gaps.is_empty(),
        "crates without forbid(unsafe_code): {gaps:?}"
    );
}

#[test]
fn nothing_in_the_repository_uses_unsafe_code() {
    let offenders = crates::files_using_unsafe(&root());
    assert!(
        offenders.is_empty(),
        "these files use unsafe code: {offenders:?}"
    );
}

#[test]
fn every_test_crate_forbids_unsafe_code_too() {
    let gaps = crates::tests_without_the_safety_attribute(&root());
    assert!(
        gaps.is_empty(),
        "test crates without forbid(unsafe_code): {gaps:?}"
    );
}

#[test]
fn a_domain_keeps_all_of_its_services_in_one_crate() {
    let split = crates::split_domains(&root());
    assert!(
        split.is_empty(),
        "a domain's services live in more than one crate: {split:?}"
    );
}

#[test]
fn the_costing_crate_carries_all_of_the_costing_services() {
    let unnamed = crates::costing_services_unnamed(&root());
    assert!(
        unnamed.is_empty(),
        "the costing crate does not name {unnamed:?}"
    );
}

#[test]
fn no_calculator_is_registered_twice() {
    // The earlier registration is dead code that looks alive: the map holds the later one and nothing says so.
    let duplicates = wiring::duplicate_calculator_registrations(&root());
    assert!(duplicates.is_empty(), "{duplicates:?}");
}

#[test]
fn no_gateway_path_or_service_name_is_listed_twice() {
    let paths = wiring::duplicate_gateway_paths(&root());
    assert!(paths.is_empty(), "two services answer one path: {paths:?}");
    let names = wiring::duplicate_service_names(&root());
    assert!(
        names.is_empty(),
        "the route table names these more than once: {names:?}"
    );
}

#[test]
fn no_two_source_files_are_identical() {
    // A byte-identical file in two crates is a copy waiting to drift.
    let copies = wiring::identical_source_files(&root());
    assert!(
        copies.is_empty(),
        "identical source files, so one is a copy: {copies:?}"
    );
}

#[test]
fn a_service_removed_as_a_duplicate_stays_removed() {
    // 179 services were removed as clones of a survivor or as calculators the Rust library answers. Their return is a
    // decision, not an accident, so the names are pinned as data and checked against the router and the cluster.
    let findings = wiring::resurrected_duplicates(&root());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn the_cluster_deploys_nothing_the_gateway_does_not_route() {
    // A live pod behind a retired address is a client's 502 that no test notices, because nothing calls it any more.
    let findings = manifests::deployed_but_unrouted(&root());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn every_probe_the_cluster_asks_for_is_answered() {
    // A liveness probe at `/` on an app with no root handler means a pod the cluster kills forever: green tests and a
    // valid manifest still add up to a service that cannot run.
    let findings = manifests::unanswerable_probes(&root());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn every_gateway_entry_forwards_to_something_that_exists() {
    let findings = manifests::entries_without_a_backend(&root());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn every_tolerated_dependency_names_an_adr_that_exists() {
    // P11.1: a gate may tolerate something, but not quietly. Each exception has to point at the decision that accepted
    // it, and the pointer has to resolve - an exception citing a document nobody wrote is a comment, not a decision.
    let decisions = root().join("docs/decisions");
    let mut checked = 0usize;
    for exception in boundaries::exceptions() {
        let Some(start) = exception.reason.find("ADR-0") else {
            panic!(
                "{} -> {} is tolerated with no ADR named: {}",
                exception.from, exception.to, exception.reason
            );
        };
        let tail = &exception.reason[start..];
        let code: String = tail
            .chars()
            .take_while(|character| *character != '-')
            .collect();
        let files: Vec<String> = std::fs::read_dir(&decisions)
            .expect("docs/decisions")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            files.iter().any(|file| file.starts_with(&code)),
            "{} is tolerated citing {code}, which no file in docs/decisions answers to ({files:?})",
            exception.from
        );
        checked += 1;
    }
    assert!(
        checked <= 1,
        "the tolerances grew to {checked}; that is a decision, not a drift"
    );
}

#[test]
fn the_cluster_is_a_base_and_some_overlays() {
    // The tree used to be flat, so "where does this go" had no answer and everything went to the top. One file in the
    // wrong place is silently a manifest either every environment gets or one environment gets.
    let findings = manifests::split_problems(&root());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn every_kustomization_names_a_file_that_exists() {
    // A resource that is not there is a `kubectl apply -k` that fails on someone else's machine.
    let findings = manifests::kustomization_problems(&root());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn every_compose_build_names_a_package_and_a_binary_the_workspace_has() {
    // The file this replaced listed forty services that had been retired into the engine, and nothing noticed because a
    // stale compose file is only visible when someone runs docker. Both are read here instead.
    let findings = manifests::compose_problems(&root());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn every_image_is_built_by_something_and_everything_named_as_an_image_exists() {
    let findings = manifests::image_problems(&root());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn a_compose_build_naming_a_binary_the_workspace_lacks_is_refused() {
    // The rule is only worth having if it can fail, so this builds a garden where it should.
    let garden = std::env::temp_dir().join(format!("vimbai-compose-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&garden);
    std::fs::create_dir_all(garden.join("infrastructure/compose")).unwrap();
    std::fs::create_dir_all(garden.join("rust/crates/engine")).unwrap();
    std::fs::write(
        garden.join("rust/crates/engine/Cargo.toml"),
        "[package]\nname = \"vimbai_api\"\n\n[[bin]]\nname = \"vimbai-api\"\n",
    )
    .unwrap();
    std::fs::write(
        garden.join("infrastructure/compose/compose.yaml"),
        "services:\n  engine:\n    build:\n      context: ../../rust\n      dockerfile: Dockerfile\n      args:\n        PACKAGE: vimbai_api\n        BIN: vimbai-gone\n",
    )
    .unwrap();
    let findings = manifests::compose_problems(&garden);
    let _ = std::fs::remove_dir_all(&garden);
    assert!(
        findings
            .iter()
            .any(|finding| finding.contains("vimbai-gone")),
        "{findings:?}"
    );
}

#[test]
fn the_workflows_name_only_services_that_are_routed() {
    // A file naming a deleted directory fails when the runner reaches that line, which is why it is checked here.
    let findings = workflows::problems(&root());
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn maths_with_no_door_is_written_down_rather_than_forgotten() {
    // Twenty-four modules once held arithmetic nothing could call: every one of them `pub` and re-exported, so every one
    // of them looked wired. Three are left, each a decision to route or delete rather than a gap nobody noticed, and the
    // scan has to keep finding them - a scanner that reaches everything is as broken as one that reaches nothing.
    let dead = reachability::unreachable_modules(&root());
    for module in [
        "tax::corporate",
        "cost_accounting::labour_variance",
        "ratios_statements::comparative",
    ] {
        assert!(
            dead.iter().any(|found| found == module),
            "{module} is no longer reported dead"
        );
    }
    assert!(
        dead.len() < 400,
        "the scan reports {} dead modules, which is every module",
        dead.len()
    );
}

#[test]
fn everything_the_port_tracker_calls_ported_is_routed() {
    // The tracker is a document, and a wrong entry in a document is worse than no entry: a substring match once counted
    // fifty-four services on the Rust engine that were not.
    let root = root();
    let routed: std::collections::BTreeSet<String> = catalog::rows(&root)
        .into_iter()
        .map(|row| row.name)
        .collect();
    let ported = catalog::ported_names(&root);
    assert!(
        !ported.is_empty(),
        "the tracker reader found no ported services"
    );
    let strays: Vec<&String> = ported
        .iter()
        .filter(|name| !routed.contains(*name))
        .collect();
    assert!(
        strays.is_empty(),
        "the tracker calls these ported and nothing routes them: {strays:?}"
    );
}

#[test]
fn each_gate_reads_something_before_it_passes() {
    // A gate over an empty set passes for the wrong reason, so the inputs are asserted: files found, services found,
    // contract paths found. This is the check the Python suite kept needing after a directory moved.
    assert!(
        workflows::names(&root()).len() >= 8,
        "the workflow scan found almost nothing"
    );
    assert!(
        !xtask::catalog::rows(&root()).is_empty(),
        "the catalogue read no services, so every rule built on it is vacuous"
    );
    assert!(
        xtask::gateway::engine_routes(&root()).len() > 1000,
        "the engine scan found almost no routes"
    );
}
