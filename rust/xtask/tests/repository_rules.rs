#![forbid(unsafe_code)]
//! The repository's own rules, held over the repository rather than over a service.
//!
//! Each rule here replaced a Python test that read the same files. They are gathered in one file for a reason: what they
//! check is the shape of the estate - where code lives, what is registered once, what the cluster deploys, what the
//! contracts promise - and a reader looking for "how is this repository kept honest" should find it in one place.

use std::path::PathBuf;

use xtask::{
    boundaries, catalog, crates, manifests, published, reachability, runbook, servers, wiring,
    workflows,
};

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
fn every_server_stands_behind_the_door() {
    let gaps = servers::servers_missing_the_door(&root());
    assert!(
        gaps.is_empty(),
        "servers serving without limits::govern: {gaps:?}"
    );
    // a gate that finds nothing because it looked nowhere is not a gate
    let found = servers::servers(&root());
    assert!(
        found.len() >= 3,
        "the sweep found {} servers, not the three this repository ships",
        found.len()
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

/// One answer to "which store is this deployment using".
///
/// The engine preferred Postgres while the worker only knew a file or Neo4j, so beside a Postgres engine the worker started
/// with no store and ran nobody's rules. The choice lives in `store::choose` now; this fails if a second copy grows back
/// anywhere else, because the next copy would drift the same way.
#[test]
fn only_the_store_chooses_which_deployment_store_to_open() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut readers = Vec::new();
    let mut stack = vec![root.join("rust/crates"), root.join("rust/apps")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) == Some("target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().and_then(|kind| kind.to_str()) == Some("rs") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                // a test may name the variable; shipped code may not read it twice
                let shipped = !path.to_string_lossy().contains("/tests/");
                if shipped && text.contains("VIMBAI_POSTGRES_URL") {
                    readers.push(
                        path.strip_prefix(&root)
                            .unwrap_or(&path)
                            .display()
                            .to_string(),
                    );
                }
            }
        }
    }
    let allowed = "rust/crates/adapters/database/src/choose.rs";
    // the store crate owns the variable: it is the thing that opens a database, and its backend names the setting it needs.
    // Nothing outside the store may read it - not the engine, not the worker, not a gate.
    readers.retain(|reader| !reader.starts_with("rust/crates/adapters/database/"));
    assert!(
        readers.is_empty(),
        "only the store crate may read VIMBAI_POSTGRES_URL; found it in {readers:?}"
    );
    // the scan has to have found the one place, or it passes over an empty set
    let chosen = std::fs::read_to_string(root.join(allowed)).expect("the one place");
    assert!(chosen.contains("VIMBAI_POSTGRES_URL"));
}

#[test]
fn the_dart_toolchain_is_pinned_and_gates_a_merge() {
    // The client is one of the platform's three languages (ADR-001), so its toolchain lives in the pipeline that gates a
    // merge, pinned to one version everywhere: `latest` is a build that means something different tomorrow.
    let problems = workflows::dart_toolchain_problems(&root());
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn the_dart_gate_would_notice_a_toolchain_that_drifted() {
    // A gate that only ever passes is not a gate. These build a tiny estate on disk: one workflow that pins a version,
    // then a second that pins another, then one that asks for `latest`, then one that only runs outside the gating
    // pipeline. The scanner has to report each.
    let scratch = std::env::temp_dir().join(format!("vimbai_dart_gate_{}", std::process::id()));
    let workflows = scratch.join(".github").join("workflows");
    std::fs::create_dir_all(&workflows).expect("a scratch directory");
    let step = |version: &str| {
        format!(
            "      - name: Set up Flutter\n        uses: flutter-actions/setup-flutter@v4\n        with:\n          channel: stable\n          version: {version}\n"
        )
    };
    let gating = |version: &str, test: bool| {
        format!(
            "name: Core\non:\n  push:\njobs:\n  test-flutter:\n    runs-on: ubuntu-latest\n    steps:\n{}      - name: Test\n        run: {}dart format --output=none --set-exit-if-changed lib test\n",
            step(version),
            if test { "flutter test\n        " } else { "" }
        )
    };
    let write = |name: &str, text: &str| {
        std::fs::write(workflows.join(name), text).expect("a workflow");
    };

    write("ci-main.yml", &gating("3.50.0", true));
    assert_eq!(
        workflows::dart_toolchain_problems(&scratch),
        Vec::<String>::new(),
        "a pinned, agreeing, gating estate has to pass"
    );

    // a gating pipeline that never starts for the app: the job is there and no app change reaches it
    write(
        "ci-main.yml",
        &gating("3.50.0", true).replace(
            "on:\n  push:\n",
            "on:\n  push:\n    paths:\n      - 'rust/**'\n      - '**/*.go'\n",
        ),
    );
    let unreachable = workflows::dart_toolchain_problems(&scratch);
    assert!(
        unreachable
            .iter()
            .any(|problem| problem.contains("never reaches")),
        "a gating pipeline filtered away from the app went unnoticed: {unreachable:?}"
    );
    // and it passes once the filter watches the app
    write(
        "ci-main.yml",
        &gating("3.50.0", true).replace(
            "on:\n  push:\n",
            "on:\n  push:\n    paths:\n      - 'apps/flutter/**'\n      - '**/*.go'\n",
        ),
    );
    assert_eq!(
        workflows::dart_toolchain_problems(&scratch),
        Vec::<String>::new(),
        "a pinned, gating pipeline that starts for the app has to pass"
    );

    // the same gate in two files: the pin and the four steps now have two places to drift
    write(
        "mobile-analyze.yml",
        &format!("name: Mobile\non:\n  push:\njobs:\n  analyze:\n    runs-on: ubuntu-latest\n    steps:\n{}", step("3.50.0")),
    );
    let doubled = workflows::dart_toolchain_problems(&scratch);
    assert!(
        doubled
            .iter()
            .any(|problem| problem.contains("mobile-analyze.yml")),
        "a second workflow installing the toolchain went unnoticed: {doubled:?}"
    );
    std::fs::remove_file(workflows.join("mobile-analyze.yml")).ok();

    // a second workflow on another version
    write(
        "mobile-analyze.yml",
        &format!("name: Mobile\non:\n  push:\njobs:\n  analyze:\n    runs-on: ubuntu-latest\n    steps:\n{}", step("3.44.0")),
    );
    let drift = workflows::dart_toolchain_problems(&scratch);
    assert!(
        drift.iter().any(|problem| problem.contains("disagree")),
        "a second pinned version went unnoticed: {drift:?}"
    );

    // `latest`
    write("ci-main.yml", &gating("latest", true));
    write(
        "mobile-analyze.yml",
        &format!("name: Mobile\non:\n  push:\njobs:\n  analyze:\n    runs-on: ubuntu-latest\n    steps:\n{}", step("latest")),
    );
    let floating = workflows::dart_toolchain_problems(&scratch);
    assert!(
        floating.iter().any(|problem| problem.contains("latest")),
        "a floating toolchain went unnoticed: {floating:?}"
    );

    // pinned, but the gating pipeline never runs the tests
    write("ci-main.yml", &gating("3.50.0", false));
    write(
        "mobile-analyze.yml",
        &format!("name: Mobile\non:\n  push:\njobs:\n  analyze:\n    runs-on: ubuntu-latest\n    steps:\n{}", step("3.50.0")),
    );
    let untested = workflows::dart_toolchain_problems(&scratch);
    assert!(
        untested
            .iter()
            .any(|problem| problem.contains("flutter test")),
        "a gating pipeline that never runs the app's tests went unnoticed: {untested:?}"
    );

    std::fs::remove_dir_all(&scratch).ok();
}

#[test]
fn the_runbook_names_settings_the_code_reads() {
    // A runbook is read while something is down. A setting renamed in the code and left behind in the page sends an
    // operator to a variable nothing reads, and they conclude the platform ignores them.
    assert!(
        !runbook::runbook_is_missing(&root()),
        "there is no runbook, so an operator with a broken engine has nothing to read"
    );
    let unknown = runbook::unknown_settings(&root());
    assert!(
        unknown.is_empty(),
        "the runbook names settings nothing reads: {unknown:?}"
    );
}

#[test]
fn the_runbook_gate_would_notice_a_setting_nobody_reads() {
    // The gate has to fail on a page that names a setting the source does not ask for, or it proves nothing. This builds a
    // scratch estate with one source file reading one setting and a runbook naming that one and a second that does not
    // exist.
    let scratch = std::env::temp_dir().join(format!("vimbai_runbook_gate_{}", std::process::id()));
    let source = scratch.join("rust").join("crate").join("src");
    let pages = scratch.join("docs").join("operations");
    std::fs::create_dir_all(&source).expect("a source directory");
    std::fs::create_dir_all(&pages).expect("a docs directory");
    std::fs::write(
        source.join("main.rs"),
        "fn main() { let _ = std::env::var(\"VIMBAI_REAL_SETTING\"); }\n",
    )
    .expect("a source file");
    std::fs::write(
        pages.join("RUNBOOK.md"),
        "Set `VIMBAI_REAL_SETTING` and never `VIMBAI_IMAGINARY_SETTING`.\n",
    )
    .expect("a runbook");

    let unknown = runbook::unknown_settings(&scratch);
    assert_eq!(
        unknown,
        vec!["VIMBAI_IMAGINARY_SETTING".to_string()],
        "the gate missed a setting nothing reads, or flagged the one that is read"
    );
    assert_eq!(
        runbook::settings_the_code_reads(&scratch)
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["VIMBAI_REAL_SETTING".to_string()]
    );
    std::fs::remove_dir_all(&scratch).ok();
}

#[test]
fn the_published_snapshot_is_what_the_engine_answers() {
    // `docs/routes-snapshot.json` is the address set a reader diffs after a change. It was written once by hand from a scan
    // of the Python services, which is the kind of document that is right on the day it is written; it is now generated
    // from the engine's own routes plus each service's last published list, and this holds it there.
    let problems = published::write(&root(), true);
    assert!(problems.is_ok(), "{problems:?}");
    let snapshot = published::snapshot(&root());
    assert!(
        snapshot.contains("\"count\": 139"),
        "the snapshot should describe the 139 routed services"
    );
}

#[test]
fn every_routed_service_answers_at_least_one_address() {
    // A prefix the gateway routes and nothing answers is worse than an absent one: a caller reaches a 404 from a door the
    // manifest says is open, and concludes the feature is broken. `/calcs` was exactly that - an entry for a service that
    // never had a `main.py`, forwarded to the engine at a path the engine never mounted - so the check is a rule now.
    let silent: Vec<String> = published::per_service(&root())
        .into_iter()
        .filter(|(_, addresses)| addresses.is_empty())
        .map(|(name, _)| name)
        .collect();
    assert!(
        silent.is_empty(),
        "these routed services answer nothing: {silent:?}"
    );
    let total: usize = published::per_service(&root()).values().map(Vec::len).sum();
    assert!(
        total > 1_700,
        "the scan found only {total} addresses, so agreeing with the snapshot proves little"
    );
}
