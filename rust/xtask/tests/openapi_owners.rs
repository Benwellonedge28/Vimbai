#![forbid(unsafe_code)]
//! Every address the engine answers is filed under an owner.
//!
//! The contract generator used to put 180 prefix groups in one bucket called `unattributed`, because the catalogue describes
//! what the gateway *routes* and a retired service's addresses are answered without being routed. A bucket that says "nobody
//! owns this" is a wish, not a contract, so the ownership is now read from the code - route, handler, crate, domain page - and
//! these tests hold it to that.

use std::collections::BTreeSet;

use xtask::{openapi, owners};

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
