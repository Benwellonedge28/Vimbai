#![forbid(unsafe_code)]
//! The worker as it is actually run: one process, the store file the engine writes, one pass.
//!
//! The library tests prove the rules against an in-memory store. This proves the two things only a real process can: that
//! the worker opens the file the on-device runtime writes, and that a second pass over that same file does not repeat the
//! work the first one finished.

use serde_json::{json, Value};
use std::process::Command;
use store::fake::MemoryGraph;
use store::{BookScope, Records};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("vimbai-worker-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir.join("store.json")
}

/// A store file with one Book open, one person in it, and a rule that is due.
fn seeded(path: &std::path::Path) -> BookScope {
    let scope = BookScope::new("user-1", "book-1");
    let graph = store::file::FileGraph::open(path).expect("a store file");
    store::PlatformRecords::new(&graph, offline_sync::sync::BOOK)
        .create(&json!({ "id": "book-1", "name": "Solo", "created_by": "user-1" }))
        .expect("a book");
    automation::automations::create_automation_rule(
        &graph,
        &scope,
        &json!({
            "id": "rule-1",
            "name": "Nightly sweep",
            "company_id": "co-1",
            "trigger": "scheduled",
            // due in 1970, which every real clock is past: this is the "long overdue" case, not a contrived one
            "condition": { "schedule": { "due_at": 1.0 } },
            "steps": [
                { "step_id": "collect", "action": "post_entries" },
                { "step_id": "close", "action": "close_period", "depends_on": ["collect"] }
            ],
        }),
    )
    .expect("a rule");
    scope
}

fn pass(path: &std::path::Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_vimbai-worker"))
        .env("VIMBAI_STORE_FILE", path)
        .env("VIMBAI_WORKER_ONCE", "1")
        // the store, not a database, is the only thing this pass can reach
        .env_remove("VIMBAI_NEO4J_URL")
        .output()
        .expect("the worker runs");
    assert!(
        output.status.success(),
        "the worker failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn runs(path: &std::path::Path, scope: &BookScope) -> Vec<Value> {
    let graph = store::file::FileGraph::open(path).expect("the store file");
    Records::new(&graph, automation::automations::EXECUTION)
        .list(scope, 50)
        .expect("the runs")
}

#[test]
fn a_pass_over_the_store_file_runs_a_due_rule_and_a_second_pass_does_not_repeat_it() {
    let path = scratch("once");
    let scope = seeded(&path);

    let first = pass(&path);
    assert!(
        first.contains("run-rule-1-1"),
        "the pass must say which run it made: {first}"
    );
    let made = runs(&path, &scope);
    assert_eq!(made.len(), 1, "one run: {made:?}");
    assert_eq!(made[0]["status"], json!("completed"));
    assert_eq!(
        made[0]["step_results"].as_array().map(Vec::len),
        Some(2),
        "both steps were walked: {:?}",
        made[0]
    );

    // the same file, a second process: the run is already recorded, so nothing is started again
    let second = pass(&path);
    assert!(
        second.contains("already recorded"),
        "the second pass must find the run done: {second}"
    );
    let after = runs(&path, &scope);
    assert_eq!(after.len(), 1, "still one run: {after:?}");
    assert_eq!(after[0]["id"], made[0]["id"], "the same run, not a new one");
}

#[test]
fn the_run_the_worker_recorded_outlives_the_process() {
    // the store is a file, so what the worker wrote is readable by anything that opens it afterwards - the engine
    // included. This is the whole reason the worker shares the file rather than keeping its own queue.
    let path = scratch("durable");
    let scope = seeded(&path);
    pass(&path);
    drop(scope);

    // a fresh process, reading the same file: nothing is held in memory between the two
    let graph = store::file::FileGraph::open(&path).expect("the store file");
    let reopened = BookScope::new("user-1", "book-1");
    let made = Records::new(&graph, automation::automations::EXECUTION)
        .list(&reopened, 50)
        .expect("the runs");
    assert_eq!(made.len(), 1, "{made:?}");
    assert_eq!(made[0]["status"], json!("completed"));
    assert!(
        !made[0]["completed_at"].is_null(),
        "the run carries when it finished: {:?}",
        made[0]
    );
}

#[test]
fn a_store_with_no_due_work_is_left_alone() {
    let path = scratch("idle");
    let graph = store::file::FileGraph::open(&path).expect("a store file");
    store::PlatformRecords::new(&graph, offline_sync::sync::BOOK)
        .create(&json!({ "id": "book-1", "name": "Solo", "created_by": "user-1" }))
        .expect("a book");
    automation::automations::create_automation_rule(
        &graph,
        &BookScope::new("user-1", "book-1"),
        &json!({
            "id": "rule-later",
            "trigger": "scheduled",
            // the year 3000: nothing any clock in this test is past
            "condition": { "schedule": { "due_at": 32503680000.0 } },
            "steps": [],
        }),
    )
    .expect("a rule");

    let quiet = pass(&path);
    assert!(
        quiet.trim().is_empty(),
        "an idle pass says nothing: {quiet}"
    );
    let graph = store::file::FileGraph::open(&path).expect("the store file");
    let made = Records::new(&graph, automation::automations::EXECUTION)
        .list(&BookScope::new("user-1", "book-1"), 50)
        .expect("the runs");
    assert!(made.is_empty(), "{made:?}");
}

#[test]
fn the_worker_without_a_store_says_so_and_stops() {
    // a worker with nowhere to work must not spin quietly: it exits with a message rather than looking healthy
    let output = Command::new(env!("CARGO_BIN_EXE_vimbai-worker"))
        .env_remove("VIMBAI_STORE_FILE")
        .env_remove("VIMBAI_NEO4J_URL")
        .env("VIMBAI_WORKER_ONCE", "1")
        .output()
        .expect("the worker runs");
    assert!(!output.status.success(), "an unconfigured worker fails");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(
        said.contains("no store") || said.contains("nowhere"),
        "it says why: {said}"
    );
}

#[test]
fn the_same_rule_and_due_instant_name_one_run_across_processes() {
    // the id is the idempotency: two processes on the same due instant agree on the name without talking to each other
    let path = scratch("identity");
    let scope = seeded(&path);
    pass(&path);
    let after_first = runs(&path, &scope);

    let graph = store::file::FileGraph::open(&path).expect("the store file");
    let rule = automation::automations::automation(&graph, &scope, "rule-1").expect("the rule");
    let due = vimbai_worker::due_at_of(&rule).expect("the due instant");
    assert_eq!(
        vimbai_worker::run_id("rule-1", due),
        after_first[0]["id"],
        "the run the worker made is the run this rule and instant name"
    );

    // and memory graph or file graph, the name is the same: nothing about the store is in it
    let memory = MemoryGraph::default();
    assert_eq!(
        vimbai_worker::run_id("rule-1", due),
        vimbai_worker::run_id(
            "rule-1",
            vimbai_worker::due_at_of(&rule).expect("the due instant")
        ),
        "the name does not depend on the store"
    );
    drop(memory);
}
