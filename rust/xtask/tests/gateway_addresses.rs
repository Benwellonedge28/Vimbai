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

use xtask::gateway::{
    audit, dead_static_routes, method_gaps, misrouted, port_drift, static_routes,
    stripped_collisions,
};

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

#[test]
fn no_prefix_that_the_engine_answers_forwards_somewhere_else() {
    // Fifteen entries kept the container port their retired service used to listen on, so the engine answered the prefix and
    // the gateway sent the call to a dead socket: a 502 from a door the manifest says is open. All fifteen are repointed now.
    // Two of them needed more than a repoint. admin-service published bare paths - `/features`, `/audit-logs` - that the
    // engine already answers, so its entry strips the prefix; bank-feed-service published `/rules`, `/transactions` and
    // `/connections`, which alerts, sales-ledger-control and banking-integration already claim as bare paths, so its entry
    // keeps the prefix and the engine mounts those routes under `/bank-feed` instead of handing a caller another service's
    // answer. There is no tolerated exception left here: any prefix the engine answers that forwards elsewhere fails.
    let findings = misrouted(&root());
    assert!(
        findings.is_empty(),
        "these prefixes are answered by the engine and forwarded elsewhere: {findings:#?}"
    );
}

#[test]
fn a_stated_port_is_the_port_the_address_names() {
    // The manifest carries a port and a url, and only the url is dialled, so the port was free to drift while still reading as
    // fact. Fourteen entries had: they forwarded to the engine and still said they listened on the container port their
    // retired service used. The check exists so the file cannot say two things about one address again.
    let findings = port_drift(&root());
    assert!(
        findings.is_empty(),
        "these entries state a port their address contradicts: {findings:#?}"
    );
}

/// The gateway's own route table sat outside every check: the address audit reads the manifest, and the manifest is not where
/// those routes were. Twenty-two of them named services retired into the engine, so the gateway published `/accounts` and
/// `/budgets` as doors to hosts that do not resolve.
#[test]
fn the_routes_the_gateway_declares_itself_are_ones_the_engine_answers() {
    let root = root();
    let routes = static_routes(&root);
    assert!(
        routes.iter().any(|(path, _)| path == "/identity"),
        "the /identity door has to be declared: signing in cannot need a token. Found {routes:#?}"
    );
    let dead = dead_static_routes(&root);
    assert!(dead.is_empty(), "{dead:#?}");
}

/// A directory shaped like the repository, with one engine route in it, so a rule that compares a door against the engine has
/// somewhere to look rather than a tree that is not there.
fn with_one_engine_route(name: &str, gateway: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!("vimbai-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("gateway/go/config")).expect("a directory");
    std::fs::create_dir_all(directory.join("rust/apps/api-server/src")).expect("a directory");
    std::fs::write(directory.join("gateway/go/config/config.go"), gateway).expect("a config");
    std::fs::write(
        directory.join("rust/apps/api-server/src/identity.rs"),
        ".route(\"/identity/users\", post(sign_in))\n",
    )
    .expect("an engine route");
    directory
}

#[test]
fn a_route_the_gateway_invents_is_reported() {
    let directory = with_one_engine_route(
        "static-routes",
        "cfg.Routes = []Route{\n\t{Path: \"/identity\", TargetURL: cfg.IdentityServiceURL},\n\t{Path: \"/accounts\", TargetURL: cfg.AccountingServiceURL},\n}\n",
    );
    let dead = dead_static_routes(&directory);
    assert_eq!(dead.len(), 1, "{dead:#?}");
    assert!(dead[0].contains("/accounts"), "{dead:#?}");
}

#[test]
fn a_gateway_with_no_door_at_all_fails_rather_than_passing() {
    let directory = with_one_engine_route("static-empty", "cfg.Routes = []Route{}\n");
    let dead = dead_static_routes(&directory);
    assert!(!dead.is_empty(), "an empty scan must fail, not pass");
}

/// A stripping entry forwards `/prefix/rest` to the engine's `/rest`, so what it reaches is decided per request and its client
/// paths are the retired service's own. That is what the published-address audit reads out of git, and it is why this rule cannot
/// reach those entries: the withdrawn version of this test listed twenty-six of them as answering nothing, which was the check
/// mis-filing a shape it had not considered rather than a defect in the catalogue.
#[test]
fn an_entry_that_strips_is_not_judged_by_its_own_path() {
    let root = root();
    let stripping = xtask::gateway::entries(&root)
        .into_iter()
        .filter(|entry| entry.strip_prefix)
        .count();
    assert!(
        stripping > 0,
        "the manifest has stripping entries; this rule has to keep accounting for them"
    );
}

/// Every entry that forwards unstripped must be an address the engine answers. A stripping entry is the audit's question, not
/// this one, so the two rules cover the two shapes between them: nothing here is a prefix the engine does not answer at the form
/// the gateway forwards it, and nothing there is a published endpoint that stops answering.
#[test]
fn every_unstripped_entry_is_an_address_the_engine_answers() {
    let root = root();
    let addressed = xtask::gateway::engine_routes(&root);
    let unanswered: Vec<String> = xtask::gateway::entries(&root)
        .into_iter()
        .filter(|entry| !entry.strip_prefix)
        .map(|entry| entry.path.trim_end_matches('/').to_string())
        .filter(|prefix| {
            !addressed
                .iter()
                .any(|address| address == prefix || address.starts_with(&format!("{prefix}/")))
        })
        .collect();
    assert!(unanswered.is_empty(), "{unanswered:#?}");
}
