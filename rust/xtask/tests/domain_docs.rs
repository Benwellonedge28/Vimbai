//! One page per domain, and every routed service in exactly one of them.
//!
//! The pages are prose, so nothing about them is compiled. That is exactly why they need a gate: a service renamed and a
//! page left alone is a reader being told something untrue with no build to contradict it.

use xtask::catalog::repo_root;
use xtask::domains;

#[test]
fn every_routed_service_has_one_page_and_no_page_is_stale() {
    let findings = domains::audit(&repo_root());
    assert!(
        findings.is_empty(),
        "{} finding(s):\n{}",
        findings.len(),
        findings.join("\n")
    );
}

#[test]
fn the_inventory_actually_has_something_to_check() {
    // a gate over an empty set passes over nothing at all, which has bitten this repository before
    let pages = domains::DOCS.len();
    assert!(
        pages >= 16,
        "the roadmap names sixteen pages at least, found {pages}"
    );
    let covered: usize = domains::DOCS.iter().map(|doc| doc.domains.len()).sum();
    assert!(covered > 20, "only {covered} domains are claimed");
}

#[test]
fn each_page_is_a_page_and_not_a_stub() {
    // a stub that merely names its services would satisfy the mapping and tell a reader nothing
    for doc in domains::DOCS {
        let path = repo_root().join("docs/domains").join(doc.file);
        let text = std::fs::read_to_string(&path).expect("a page");
        assert!(
            text.lines().count() >= 20,
            "{} is {} lines: too thin to be the page for {:?}",
            doc.file,
            text.lines().count(),
            doc.domains
        );
        assert!(
            text.contains("What it does not own"),
            "{} does not say what it does not own",
            doc.file
        );
        assert!(
            text.contains("## Tests"),
            "{} does not say what proves it",
            doc.file
        );
    }
}

#[test]
fn no_page_covers_the_same_domain_as_another() {
    let doubled = domains::doubled();
    assert!(doubled.is_empty(), "{}", doubled.join("\n"));
}

#[test]
fn the_payments_page_states_the_decision_that_it_is_pos_only() {
    // ADR-005: the page that would otherwise invite a reader to expect a wallet must refuse the expectation
    let text =
        std::fs::read_to_string(repo_root().join("docs/domains/payments.md")).expect("the page");
    assert!(
        text.contains("ADR-005"),
        "payments.md must cite the decision"
    );
    assert!(
        text.to_lowercase().contains("vimbai pay is not built"),
        "payments.md must state plainly that Vimbai Pay is not built"
    );
    assert!(
        text.contains("pos-integration"),
        "payments.md must name the till it does cover"
    );
}
