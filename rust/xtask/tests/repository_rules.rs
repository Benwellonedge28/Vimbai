//! The repository's own rules, held over the repository rather than over a service.
//!
//! Each rule here replaced a Python test that read the same files. They are gathered in one file for a reason: what they
//! check is the shape of the estate - where code lives, what is registered once, what the cluster deploys, what the
//! contracts promise - and a reader looking for "how is this repository kept honest" should find it in one place.

use std::path::PathBuf;

use xtask::{catalog, crates, manifests, reachability, wiring, workflows};

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
