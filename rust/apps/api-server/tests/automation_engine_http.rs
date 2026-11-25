#![forbid(unsafe_code)]
//! automation-engine-service over HTTP: a rule, its switch, and the runs it produced.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use store::fake::MemoryGraph;
use tower::ServiceExt;

fn router() -> axum::Router {
    vimbai_api::records::router(Arc::new(MemoryGraph::new()))
}

async fn call(
    router: &axum::Router,
    method: Method,
    uri: &str,
    payload: Value,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-user-id", "user-1")
        .header("x-book-id", "book-1")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn a_rule_runs_its_steps_and_a_broken_dependency_stops_it() {
    let router = router();

    let (status, made) = call(
        &router,
        Method::POST,
        "/automation-rules",
        json!({"name": "Month end sweep", "company_id": "co-1", "trigger": "scheduled",
        "steps": [
            {"step_id": "s1", "step_name": "Charge depreciation", "action": "post_depreciation",
             "params": {"period": "2026-10"}},
            {"step_id": "s2", "step_name": "Close October", "action": "close_period",
             "params": {"period": "2026-10"}, "depends_on": ["s1"]}
        ]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["enabled"], json!(true));
    assert_eq!(made["trigger"], json!("scheduled"));
    let id = made["id"].as_str().expect("an id").to_string();

    // a trigger nobody agreed on is a bad request
    let (status, _) = call(
        &router,
        Method::POST,
        "/automation-rules",
        json!({"name": "x", "trigger": "whenever"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // the run answers finished, with every step recorded in order
    let (status, run) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{id}/execute"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    assert_eq!(run["status"], json!("completed"), "{run}");
    assert_eq!(run["step_results"].as_array().map(Vec::len), Some(2));
    assert_eq!(run["error"], Value::Null);
    // the steps did something and said what: the depreciation step answered for the month it charged
    assert_eq!(
        run["step_results"][0]["result"]["period"],
        json!("2026-10"),
        "{run}"
    );

    // switching the rule off refuses the next run rather than quietly doing nothing
    let (status, off) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{id}/toggle"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{off}");
    assert_eq!(off["enabled"], json!(false));
    let (status, refused) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{id}/execute"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    let (_, _) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{id}/toggle"),
        json!({}),
    )
    .await;

    // a step naming work nothing can do stops the run in words rather than passing for done
    let (_, unknown) = call(
        &router,
        Method::POST,
        "/automation-rules",
        json!({"name": "Payroll sweep", "company_id": "co-1",
               "steps": [{"step_id": "s1", "step_name": "Pay everyone", "action": "post_payroll"}]}),
    )
    .await;
    let unknown_id = unknown["id"].as_str().expect("an id").to_string();
    let (status, refused_step) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{unknown_id}/execute"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refused_step}");
    assert_eq!(refused_step["status"], json!("failed"));
    let reason = refused_step["error"].as_str().unwrap_or_default();
    assert!(reason.contains("post_payroll"), "{reason}");
    assert!(reason.contains("post_depreciation"), "{reason}");

    // a rule waiting on a step that never ran is recorded as failed, with the reason kept
    let (_, broken) = call(
        &router,
        Method::POST,
        "/automation-rules",
        json!({"name": "Broken", "company_id": "co-1",
               "steps": [{"step_id": "s1", "step_name": "Report", "action": "report",
                          "depends_on": ["gone"]}]}),
    )
    .await;
    let broken_id = broken["id"].as_str().expect("an id").to_string();
    let (status, failed) = call(
        &router,
        Method::POST,
        &format!("/automation-rules/{broken_id}/execute"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    assert_eq!(failed["status"], json!("failed"));
    assert_eq!(
        failed["error"],
        json!("Dependency gone not completed for step s1")
    );

    // the runs are readable, narrowed by status, and one by id
    let (_, runs) = call(
        &router,
        Method::GET,
        "/automation-executions?status=failed",
        json!({}),
    )
    .await;
    // two runs failed: the rule nothing could do, and the one waiting on a step that never ran
    assert_eq!(runs.as_array().map(Vec::len), Some(2));
    let run_id = run["id"].as_str().expect("an id").to_string();
    let (status, found) = call(
        &router,
        Method::GET,
        &format!("/automation-executions/{run_id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found["id"], json!(run_id));
    let (status, _) = call(
        &router,
        Method::GET,
        "/automation-executions/nope",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // deleting answers what it found, and a second delete answers false rather than missing
    let (status, gone) = call(
        &router,
        Method::DELETE,
        &format!("/automation-rules/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{gone}");
    assert_eq!(gone["deleted"], json!(true));
    let (status, again) = call(
        &router,
        Method::DELETE,
        &format!("/automation-rules/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["deleted"], json!(false));
    let (status, _) = call(
        &router,
        Method::GET,
        &format!("/automation-rules/{id}"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The month a Book's clock work closes is a month a late bill can still reach, because reopening is an act with a
/// record rather than a setting nobody can find.
#[tokio::test]
async fn a_month_the_clock_work_closed_is_reopened_through_the_route_and_the_late_entry_belongs_in_it(
) {
    let router = router();
    let (status, closed) = call(
        &router,
        Method::POST,
        "/accounting/closed-periods",
        json!({"period": "2026-10", "closes_on": "2026-11-01", "closed_by": "the clock"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{closed}");

    let (_, periods) = call(
        &router,
        Method::GET,
        "/accounting/closed-periods",
        json!({}),
    )
    .await;
    assert_eq!(periods["closed_months"], json!(["2026-10"]), "{periods}");

    // the way out is named by the door that refuses, and it is one request away
    let (status, reopened) = call(
        &router,
        Method::POST,
        "/accounting/closed-periods/reopen",
        json!({
            "period": "2026-10",
            "reopened_on": "2026-11-03",
            "reopened_by": "Samuel",
            "reason": "a supplier bill arrived late",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reopened}");

    let (_, after) = call(
        &router,
        Method::GET,
        "/accounting/closed-periods",
        json!({}),
    )
    .await;
    assert_eq!(after["closed_months"], json!([]), "{after}");
    // the close is still on the record, with the reopening and its reason beside it
    assert_eq!(
        after["closed"][0]["closes_on"],
        json!("2026-11-01"),
        "{after}"
    );
    assert_eq!(
        after["closed"][0]["reopened_on"],
        json!("2026-11-03"),
        "{after}"
    );
    assert_eq!(
        after["closed"][0]["reopen_reason"],
        json!("a supplier bill arrived late"),
        "{after}"
    );

    // a reopening with no reason is refused, because the record is the point of it
    let (status, refused) = call(
        &router,
        Method::POST,
        "/accounting/closed-periods/reopen",
        json!({"period": "2026-10", "reopened_on": "2026-11-04", "reason": ""}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}
