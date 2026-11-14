//! The dependency rules are enforced here rather than remembered.
//!
//! Each test answers one question a reader would otherwise have to take on trust: that the graph was actually read, that
//! the rules hold today, that a forbidden dependency would be caught, that a tolerance still describes something real,
//! and that the documentation names the same rules the code enforces.

use std::collections::BTreeSet;

use xtask::boundaries::{self, Family};

fn root() -> std::path::PathBuf {
    boundaries_root()
}

fn boundaries_root() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(std::path::Path::parent)
        .expect("xtask lives at rust/xtask")
        .to_path_buf()
}

#[test]
fn the_scan_finds_every_crate_and_every_family() {
    let crates = boundaries::graph(&root());
    assert!(
        crates.len() >= 60,
        "the workspace had 63 crates when this gate was written, found {}; \
         a scan that finds nothing must fail rather than pass over an empty set",
        crates.len()
    );
    for family in [
        Family::Common,
        Family::Kernel,
        Family::Calculations,
        Family::Platform,
        Family::Domain,
        Family::Adapters,
        Family::Application,
        Family::Apps,
        Family::Tooling,
    ] {
        assert!(
            crates.iter().any(|item| item.is(family)),
            "no crate was found in the {} family, so that family's rule is untested",
            family.label()
        );
    }
    let edges: usize = crates.iter().map(|item| item.dependencies.len()).sum();
    assert!(
        edges >= 150,
        "only {edges} internal dependencies were read; the manifests were not parsed"
    );
}

#[test]
fn a_renamed_dependency_is_still_a_dependency() {
    // `rust/apps/api-server` names its dependencies by alias: `accounting = { package = "vimbai_service_accounting" }`.
    // Reading the alias as a crate name would invent a crate and miss the real edge.
    let crates = boundaries::graph(&root());
    let api = crates
        .iter()
        .find(|item| item.name == "vimbai_api")
        .expect("the API server is a crate in this workspace");
    assert!(
        api.dependencies.contains("vimbai_service_accounting"),
        "the API server's aliased dependency on the accounting domain was not resolved; read {:?}",
        api.dependencies
    );
    assert!(
        !api.dependencies.contains("accounting"),
        "the alias itself was recorded as a crate name"
    );
}

#[test]
fn no_crate_breaks_a_dependency_rule() {
    let crates = boundaries::graph(&root());
    let violations = boundaries::violations_of(&crates);
    assert!(
        violations.is_empty(),
        "{} dependency rule violations:\n{}",
        violations.len(),
        violations
            .iter()
            .map(|item| item.report())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn every_crate_belongs_to_a_family() {
    let crates = boundaries::graph(&root());
    let unclassified = boundaries::unclassified(&crates);
    assert!(
        unclassified.is_empty(),
        "crates outside every family have no rule applied to them:\n{}",
        unclassified.join("\n")
    );
}

#[test]
fn no_library_depends_on_a_binary() {
    let crates = boundaries::graph(&root());
    let findings = boundaries::depending_on_an_app(&crates);
    assert!(
        findings.is_empty(),
        "a library reached into a program:\n{}",
        findings.join("\n")
    );
}

#[test]
fn the_gate_catches_a_calculation_importing_a_domain() {
    // The rule table is applied to a crate built here rather than to the workspace: a gate nobody has seen fail is a
    // gate nobody should trust.
    let calculation = boundaries::Crate {
        name: "vimbai_cost_accounting".to_string(),
        path: "rust/crates/calculations/cost_accounting".to_string(),
        family: Some(Family::Calculations),
        dependencies: BTreeSet::from(["vimbai_service_costing".to_string()]),
        dev_dependencies: BTreeSet::new(),
    };
    let domain = boundaries::Crate {
        name: "vimbai_service_costing".to_string(),
        path: "rust/crates/domains/costing".to_string(),
        family: Some(Family::Domain),
        dependencies: BTreeSet::new(),
        dev_dependencies: BTreeSet::new(),
    };
    let found = boundaries::violations_of(&[calculation, domain]);
    assert_eq!(
        found.len(),
        1,
        "a calculation importing a domain must be refused"
    );
    assert_eq!(found[0].rule, "R3");

    // And the same pairing is refused the other way round: one domain importing another.
    let accounting = boundaries::Crate {
        name: "vimbai_service_accounting".to_string(),
        path: "rust/crates/domains/accounting".to_string(),
        family: Some(Family::Domain),
        dependencies: BTreeSet::from(["vimbai_service_costing".to_string()]),
        dev_dependencies: BTreeSet::new(),
    };
    let costing = boundaries::Crate {
        name: "vimbai_service_costing".to_string(),
        path: "rust/crates/domains/costing".to_string(),
        family: Some(Family::Domain),
        dependencies: BTreeSet::new(),
        dev_dependencies: BTreeSet::new(),
    };
    let found = boundaries::violations_of(&[accounting, costing]);
    assert_eq!(
        found.len(),
        1,
        "one domain importing another must be refused"
    );
    assert_eq!(found[0].rule, "R5");
}

#[test]
fn every_tolerated_dependency_is_still_there() {
    let crates = boundaries::graph(&root());
    let stale = boundaries::stale_exceptions(&crates);
    assert!(
        stale.is_empty(),
        "a tolerance has outlived the dependency it describes:\n{}",
        stale.join("\n")
    );
}

#[test]
fn the_ledger_authority_is_defined_in_one_place() {
    let findings = boundaries::ledger_authority_outside_the_kernel(&root());
    assert!(
        findings.is_empty(),
        "posting integrity is decided outside the kernel:\n{}",
        findings.join("\n")
    );
}

#[test]
fn the_documentation_names_every_rule_the_code_enforces() {
    let path = root().join("docs/architecture/dependency-rules.md");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|problem| {
        panic!(
            "{} is the document this gate is checked against: {problem}",
            path.display()
        );
    });
    for rule in boundaries::rules() {
        assert!(
            text.contains(rule.id),
            "{} ({}) is enforced in code and missing from {}",
            rule.id,
            rule.from.label(),
            path.display()
        );
    }
    for exception in boundaries::exceptions() {
        assert!(
            text.contains(exception.from) && text.contains(exception.to),
            "the tolerated dependency {} -> {} must be named in {} alongside its reason",
            exception.from,
            exception.to,
            path.display()
        );
    }
    // The document must not promise a rule the code does not enforce.
    let documented: BTreeSet<&str> = boundaries::rules().iter().map(|rule| rule.id).collect();
    for candidate in ["R1", "R2", "R3", "R4", "R5", "R6", "R7", "R8", "R9", "R10"] {
        if text.matches(candidate).count() > 0 && !documented.contains(candidate) {
            panic!(
                "{candidate} appears in {} but no rule by that name is enforced",
                path.display()
            );
        }
    }
}
