#![forbid(unsafe_code)]
//! The gateway's promises, checked against the engine that answers them.
//!
//! Four rules, each one paid for by a real defect:
//!
//! * an address a service published must answer at the form its gateway entry forwards;
//! * it must answer for the method it published, not just at the address;
//! * the engine's excuse list must not outlive the gaps it excused;
//! * two stripped services may not claim one address, because the second caller silently gets the first service's answer.
//!
//! These read the retired services' last `main.py` out of git, so the check is against what the gateway promised rather
//! than against what the router happens to say today.

use xtask::gateway::{audit, method_gaps, stripped_collisions};

fn root() -> std::path::PathBuf {
    xtask::catalog::repo_root()
}

#[test]
fn the_engine_answers_every_address_a_retired_service_published() {
    let findings = audit(&root());
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}

#[test]
fn every_published_method_is_answered_not_just_the_address() {
    // A path answered at the right address and the wrong verb still fails the caller: a listing published as GET while
    // the engine binds only POST passed the address check for as long as nobody called it.
    let findings = method_gaps(&root());
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}

#[test]
fn two_engine_served_services_never_claim_one_stripped_path() {
    // One engine means one handler per address. Two stripped services both publishing `/stats` answers a 200 with the
    // other service's numbers, which is worse than a 404, so any collision is a failure and not a pinned list.
    let collisions = stripped_collisions(&root());
    assert!(
        collisions.is_empty(),
        "two engine-served services claim one stripped path: {collisions:?}"
    );
}

#[test]
fn a_parameter_is_not_a_different_address() {
    // The scanners blank `{id}` and `:id` alike, so a route written in axum's colon form answers a service that
    // published the brace form. Nine services drifted on exactly this before it was checked.
    use xtask::gateway::shape;
    assert_eq!(
        shape("/accounts/{number}/postings"),
        "/accounts/{}/postings"
    );
    assert_eq!(shape("/accounts/:number/postings"), "/accounts/{}/postings");
    assert_eq!(shape("/stats/"), "/stats");
}

#[test]
fn the_scan_reads_the_engine_it_claims_to_read() {
    // A gate over an empty set passes for the wrong reason. If the route scanner stops finding routes, every rule above
    // still goes green while checking nothing, so the scan itself is asserted.
    let routes = xtask::gateway::engine_routes(&root());
    assert!(
        routes.len() > 1000,
        "the engine scan found {} addresses, which cannot be the whole router",
        routes.len()
    );
    assert!(routes.contains("/accounting/accounts"));
    assert!(routes.contains("/records/accounting/journal-entries"));
}
