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
    // Nothing is unanswered any more: `/ratio-analysis` and `/household-finance` were the two addresses the app called that
    // nothing served, and both now ask the engine's calculator door at `/calc`, which is routed and holds the one copy of
    // each calculation. A list of tolerated gaps would only be a place for the next one to hide.
    let unaccounted: Vec<&String> = prefixes
        .iter()
        .filter(|prefix| !routed.contains(prefix) && !OPEN.contains(&prefix.as_str()))
        .collect();
    assert!(
        unaccounted.is_empty(),
        "the app calls prefixes nothing serves: {unaccounted:?}"
    );
}
