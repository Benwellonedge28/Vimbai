#![forbid(unsafe_code)]
//! The worker against the store a deployment actually uses.
//!
//! The engine prefers Postgres, and for a while the worker did not: it knew a store file and Neo4j, so beside a Postgres
//! engine it started with no store at all and ran nobody's rules - a deployment where the schedules silently stop. Both
//! binaries now ask `store::choose` which store is configured, and this proves the worker's half of that over a real
//! Postgres with Neo4j deliberately pointed at a dead address, so a passing run cannot be Neo4j's doing.
//!
//! Skips without `VIMBAI_TEST_POSTGRES`, which preflight exports after starting a throwaway server.

use serde_json::{json, Value};
use std::process::Command;
use store::{BookScope, Records};

fn dsn() -> Option<String> {
    std::env::var("VIMBAI_TEST_POSTGRES")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn seed(scope: &BookScope, graph: &store::choose::GraphHandle) {
    store::PlatformRecords::new(graph.as_ref(), offline_sync::sync::BOOK)
        .create(&json!({
            "id": scope.book_id,
            "name": "Deployment Book",
            "created_by": scope.user_id,
        }))
        .expect("a book");
    automation::automations::create_automation_rule(
        graph.as_ref(),
        scope,
        &json!({
            "id": format!("rule-deployment-{}", std::process::id()),
            "name": "Deployment sweep",
            "company_id": "co-1",
            "trigger": "scheduled",
            // due in 1970: every real clock is past it, so this is the overdue case rather than a contrived one
            "condition": { "schedule": { "due_at": 1.0 } },
            "steps": [{ "step_id": "collect", "action": "post_entries" }],
        }),
    )
    .expect("a rule");
}

fn pass(dsn: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_vimbai-worker"))
        .env("VIMBAI_POSTGRES_URL", dsn)
        // a second, reachable-looking store must not be what answers
        .env("NEO4J_URI", "http://127.0.0.1:1")
        .env("VIMBAI_WORKER_ONCE", "1")
        .env_remove("VIMBAI_STORE_FILE")
        .output()
        .expect("the worker runs");
    assert!(
        output.status.success(),
        "the worker failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn runs(graph: &store::choose::GraphHandle, scope: &BookScope) -> Vec<Value> {
    Records::new(graph.as_ref(), automation::automations::EXECUTION)
        .list(scope, 50)
        .expect("the runs")
}

#[test]
fn a_due_rule_runs_in_the_deployment_store_and_a_second_pass_does_not_repeat_it() {
    let Some(dsn) = dsn() else {
        eprintln!("skipping: VIMBAI_TEST_POSTGRES is not set");
        return;
    };
    std::env::set_var("VIMBAI_POSTGRES_URL", &dsn);
    std::env::set_var("NEO4J_URI", "http://127.0.0.1:1");
    let scope = BookScope::new(
        format!("user-deployment-{}", std::process::id()),
        format!("book-deployment-{}", std::process::id()),
    );
    let graph = store::choose::deployment_from_env().expect("the deployment store");
    seed(&scope, &graph);

    let first = pass(&dsn);
    assert!(
        first.contains("run-rule-deployment"),
        "the pass must say which run it made: {first}"
    );
    let made = runs(&graph, &scope);
    assert_eq!(made.len(), 1, "one run: {made:?}");
    assert_eq!(made[0]["status"], json!("completed"));

    let second = pass(&dsn);
    assert!(
        second.contains("already recorded"),
        "the second pass must find the run done: {second}"
    );
    assert_eq!(runs(&graph, &scope).len(), 1, "the same run, not a new one");
}
