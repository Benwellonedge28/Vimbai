#![forbid(unsafe_code)]
//! The addresses the app calls are addresses the platform serves.
//!
//! The app builds every URL from one constant, and nothing checked that the prefix after it was one the gateway routes. Two were
//! not: the budget surfaces moved to `/budgeting` while the app still called `/budget`, and `/ratio-analysis` is a prefix no
//! entry has ever routed - a screen that comes back 404 and reads like a bug in the screen.

use xtask::app_calls;

fn root() -> std::path::PathBuf {
    xtask::catalog::repo_root()
}

#[test]
fn every_prefix_the_app_calls_is_routed_or_open() {
    let prefixes = app_calls::prefixes(&root());
    assert!(
        prefixes.len() >= 5,
        "the scan found {} prefixes, which is too few to be the app's calls: {prefixes:?}",
        prefixes.len()
    );
    let routed: Vec<String> = xtask::catalog::rows(&root())
        .into_iter()
        .map(|row| row.gateway_path.trim_start_matches('/').to_string())
        .collect();
    // `/identity` is an open door in the gateway rather than a routed prefix, and has to be: a caller cannot present a token
    // before the door that issues one has answered. `/health` is the other one, for the same reason.
    const OPEN: [&str; 2] = ["identity", "health"];
    // Two addresses the app calls that nothing serves, both needing a decision rather than a rename, so both are named here -
    // the list cannot grow quietly and cannot be fixed quietly either.
    //
    // `/ratio-analysis`: the engine computes ratios from the Book's stored sheet at `GET /balance-sheet/ratios/{company_id}`,
    // which is routed. The app instead POSTs a balance sheet it worked out itself, so moving it changes the contract.
    //
    // `/household-finance`: the app asks for `/analyze`. The engine's household surface is `/personal-finance` - debts, goals,
    // income, investments and an `/overview` - and has no `/analyze`, so the screen needs pointing at the addresses that exist
    // rather than at a service that was never ported.
    const UNANSWERED: [&str; 2] = ["ratio-analysis", "household-finance"];
    let unaccounted: Vec<&String> = prefixes
        .iter()
        .filter(|prefix| !routed.contains(prefix) && !OPEN.contains(&prefix.as_str()))
        .filter(|prefix| !UNANSWERED.contains(&prefix.as_str()))
        .collect();
    assert!(
        unaccounted.is_empty(),
        "the app calls prefixes nothing serves: {unaccounted:?}"
    );
}
