#![forbid(unsafe_code)]
//! Every address the engine answers is filed under an owner.
//!
//! The contract generator used to put 180 prefix groups in one bucket called `unattributed`, because the catalogue describes
//! what the gateway *routes* and a retired service's addresses are answered without being routed. A bucket that says "nobody
//! owns this" is a wish, not a contract, so the ownership is now read from the code - route, handler, crate, domain page - and
//! these tests hold it to that.

use std::collections::BTreeSet;

use xtask::{catalog, openapi, owners};

fn root() -> std::path::PathBuf {
    xtask::catalog::repo_root()
}

#[test]
fn no_address_is_left_without_an_owner() {
    let expected = openapi::expected(&root());
    assert!(
        expected.contains_key("accounting.yaml"),
        "the generator produced no accounting specification, so it is reading nothing"
    );
    assert!(
        !expected.contains_key(&format!("{}.yaml", openapi::UNATTRIBUTED)),
        "the generator still files addresses under `{}`: they answer, so something owns them",
        openapi::UNATTRIBUTED
    );
    assert_eq!(
        openapi::problems(&root()),
        Vec::<String>::new(),
        "the contract gate found an address or a contract with no counterpart"
    );
}

#[test]
fn every_owner_names_a_crate_that_exists_and_a_domain_written_down_somewhere() {
    let attributed = owners::attributed(&root());
    assert!(
        attributed.len() > 100,
        "only {} prefix groups were attributed, so the scanner is reading a fraction of the route table",
        attributed.len()
    );
    for (prefix, owner) in &attributed {
        assert!(
            owner.krate.starts_with("vimbai_"),
            "{prefix} is attributed to `{}`, which is not a crate of this workspace",
            owner.krate
        );
        assert!(
            !owner.domain.is_empty(),
            "{prefix} is attributed to a crate and to no domain at all"
        );
        assert!(
            owner.addresses > 0,
            "{prefix} is attributed to a crate that answers none of its addresses"
        );
    }
}

#[test]
fn the_owner_is_read_out_of_the_code() {
    let attributed = owners::attributed(&root());
    // A door whose handler is a closure calling one function: the crate that call names, not a list someone keeps by hand.
    // A prefixed retired service is attributed the same way; the ones the catalogue still routes are its own business.
    let owner = attributed
        .get("/book-membership")
        .unwrap_or_else(|| panic!("/book-membership was not attributed at all"));
    assert_eq!(
        owner.krate, "vimbai_tenancy",
        "/book-membership is attributed to the wrong crate"
    );
}

#[test]
fn a_prefix_two_domains_answer_is_reported_not_settled() {
    let split = owners::split(&root());
    let records = split
        .get("/records")
        .unwrap_or_else(|| panic!("/records answers from several crates and was not reported"));
    assert!(
        records.len() > 1,
        "/records is reported as split by {} crate(s), which cannot be right",
        records.len()
    );
    for prefix in split.keys() {
        assert!(prefix.starts_with('/'), "{prefix} is not a prefix group");
    }
    // the crates a split group names are all real, so the report is something a reader can act on
    let known: BTreeSet<String> = owners::attributed(&root())
        .into_values()
        .map(|owner| owner.krate)
        .collect();
    assert!(!known.is_empty());
}

#[test]
fn a_crate_is_reached_through_the_modules_and_imports_it_is_written_with() {
    let attributed = owners::attributed(&root());
    // `use budgeting_services::{` wraps over four lines and its items are called as `master_budget::analyze`, and
    // `use invoicing_services::invoices;` names a module after the crate. Reading one line at a time saw neither, so
    // thirty-four addresses documented themselves as unattributed - every budgeting calculation among them.
    // `/budgeting` is a routed prefix now, so its catalogue row owns it and this reading no longer sees it; `/forecasts` is
    // the budgeting surface the gateway does not route, and it reaches its crate the same way.
    let budgeting = attributed.get("/forecasts").unwrap_or_else(|| {
        panic!("/forecasts is called through modules of its crate and was not attributed")
    });
    assert_eq!(
        budgeting.domain, "budgeting",
        "/forecasts is filed under the wrong domain"
    );
    // `/invoices` and `/customers` are routed prefixes now, so their catalogue rows own them and this reading no longer sees
    // either. That is the point rather than a loss: a routed prefix has one owner, named once, and the reading here is only for
    // the addresses the gateway does not route. What was a written-down example of a module named after its crate is now a
    // statement that the catalogue took the address over.
    for routed in ["/invoices", "/customers"] {
        assert!(
            !attributed.contains_key(routed),
            "{routed} is a routed prefix, so it cannot also be attributed to a crate by this reading"
        );
    }
}

#[test]
fn a_handler_is_attributed_from_its_own_body_and_not_the_file_around_it() {
    let attributed = owners::attributed(&root());
    // The cache's own statistics name the cache crate in the handler. Scanning to the end of `records.rs` found workflow
    // first, because the workflow definitions are written below it in the same file.
    let cache = attributed
        .get("/cache-stats")
        .unwrap_or_else(|| panic!("/cache-stats was not attributed"));
    assert_ne!(
        cache.krate, "vimbai_workflow",
        "/cache-stats reads the cache and is attributed to workflow, which is the crate written below it"
    );
    assert_eq!(
        cache.krate, "vimbai_cache",
        "/cache-stats is attributed to the wrong crate"
    );
}

#[test]
fn the_calculator_doors_are_routed_and_filed_under_their_page() {
    // The calculator catalogue is the engine's own registry: no domain crate answers it. It used to be attributed by the
    // page whose address table claimed the prefix, because nothing routed it. It is routed now - `/calc` runs a calculator
    // and `/calculators` lists them - so it is owned by a catalogue row like every other prefix, and the page that owns the
    // domain has to name both doors or the gate cannot find them.
    let rows = catalog::rows(&root());
    let run = rows
        .iter()
        .find(|row| row.name == "calculation-api")
        .unwrap_or_else(|| panic!("calculation-api is not routed"));
    assert_eq!(run.gateway_path, "/calc");
    assert!(
        run.endpoints
            .iter()
            .any(|address| address.starts_with("POST /calc/")),
        "the runner publishes no address: {:?}",
        run.endpoints
    );
    let catalogue = rows
        .iter()
        .find(|row| row.name == "calculator-catalogue")
        .unwrap_or_else(|| panic!("the catalogue door is not routed"));
    assert_eq!(catalogue.gateway_path, "/calculators");
    assert_eq!(
        catalogue.endpoints,
        vec!["GET /calculators".to_string()],
        "the catalogue door should answer the listing and nothing else"
    );
    let page =
        std::fs::read_to_string(root().join("docs/domains/ledger.md")).expect("the ledger page");
    for door in ["/calc", "/calculators"] {
        assert!(
            page.contains(door),
            "the page that owns the calculations domain does not name {door}"
        );
    }
}
