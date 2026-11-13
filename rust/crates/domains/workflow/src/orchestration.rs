#![forbid(unsafe_code)]
//! distributed-workflow-orchestration-service: multi-step workflows that run in order, and the rollback of what ran.
//!
//! A workflow is a name, a description and an ordered list of steps; each step names the service and endpoint it belongs
//! to, the method and payload it carries, and separately the compensation to run if the workflow is rolled back.
//!
//! What this crate does NOT do is keep as shipped, and it matters enough to say plainly: the shipped service never called
//! the services its steps name. Running a workflow walked the step list and marked each step completed, one after another,
//! without making a single request. The port keeps that, because inventing calls the shipped service never made would be a
//! much larger change than a port, and because the compensations it would then have to undo were never performed either.
//! What the port does instead is record the run honestly: the steps are marked completed, the index of the last step is
//! kept, and the answer counts the whole step list rather than pretending a subset ran.
//!
//! The rules that decide who may do what are the service's own, and all are kept:
//!
//! * a workflow runs only from `pending` or `failed`. A workflow that is already running, completed, compensated or
//!   cancelled answers a bad request, because running a completed workflow again would mark its steps completed a second
//!   time and lose the fact that it ever finished;
//! * a compensation walks the steps backwards, and only a step that completed AND carries a compensation address is
//!   rolled back. A step with no compensation has nothing to undo, so it stays completed rather than being labelled
//!   compensated;
//! * cancelling is unconditional. A completed workflow can be cancelled, which is shipped behaviour rather than an
//!   oversight: cancelling is how a caller closes a workflow out, and refusing it for some states only leaves records
//!   that can never be closed.
//!
//! Nothing here sends a request anywhere. That is stated in every answer that could be mistaken for one.

use serde_json::{json, Value};

use vimbai_common::ids::service_id;
use vimbai_store::read::{number, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A workflow and its steps.
pub const WORKFLOW: RecordKind =
    RecordKind::new("OrchestratedWorkflow", "OWNS_ORCHESTRATED_WORKFLOW");

/// The six states a workflow can be in.
pub const WORKFLOW_STATES: [&str; 6] = [
    "pending",
    "running",
    "completed",
    "failed",
    "compensated",
    "cancelled",
];

/// The states a workflow may be run from.
const RUNNABLE: [&str; 2] = ["pending", "failed"];

fn stamp(payload: &Value) -> Value {
    match payload.get("now") {
        Some(stated) if !stated.is_null() => stated.clone(),
        _ => json!(vimbai_common::clock::moment()),
    }
}

/// One step, with every default the shipped service gave it.
fn step_of(given: &Value) -> Value {
    json!({
        "id": if text(given, "id").is_empty() { service_id() } else { text(given, "id") },
        "name": text(given, "name"),
        "service_url": text(given, "service_url"),
        "endpoint": text(given, "endpoint"),
        "method": if text(given, "method").is_empty() { "POST".to_string() } else { text(given, "method") },
        "payload": given.get("payload").cloned().unwrap_or_else(|| json!({})),
        "compensation_url": text(given, "compensation_url"),
        "max_retries": match given.get("max_retries") {
            Some(_) => number(given, "max_retries") as i64,
            None => 3,
        },
        "retry_count": match given.get("retry_count") {
            Some(_) => number(given, "retry_count") as i64,
            None => 0,
        },
        "status": if text(given, "status").is_empty() { "pending".to_string() } else { text(given, "status") },
        "started_at": given.get("started_at").cloned().unwrap_or(Value::Null),
        "completed_at": given.get("completed_at").cloned().unwrap_or(Value::Null),
        "error_message": text(given, "error_message"),
    })
}

/// `POST /workflows`: record a workflow.
pub fn create_orchestration(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let steps: Vec<Value> = payload
        .get("steps")
        .and_then(Value::as_array)
        .map(|given| given.iter().map(step_of).collect())
        .unwrap_or_default();
    let moment = stamp(payload);
    Records::new(graph, WORKFLOW).create(
        scope,
        &json!({
            "id": if text(payload, "id").is_empty() { service_id() } else { text(payload, "id") },
            "name": text(payload, "name"),
            "description": text(payload, "description"),
            "steps": steps,
            // a new workflow has run nothing, so it is pending with its index at the first step
            "status": "pending",
            "current_step_index": 0,
            "created_at": moment,
            "updated_at": moment,
            "created_by": text(payload, "created_by"),
        }),
    )
}

/// `GET /workflows`: the Book's workflows, narrowed by status when asked.
pub fn orchestrations(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let wanted = text(payload, "status");
    let rows = Records::new(graph, WORKFLOW).list(scope, 500)?;
    Ok(Value::Array(
        rows.into_iter()
            .filter(|row| wanted.is_empty() || text(row, "status") == wanted)
            .collect(),
    ))
}

/// `GET /workflows/{id}`: one workflow.
pub fn orchestration(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    workflow_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, WORKFLOW).get(scope, workflow_id)
}

/// `POST /workflows/{id}/execute`: walk the steps in order and record them as taken.
///
/// No request is sent to the services the steps name. See the note at the top of this module: the shipped service
/// simulated the run, and the port says so rather than pretending otherwise.
pub fn run_orchestration(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    workflow_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = orchestration(graph, scope, workflow_id)?;
    let state = text(&held, "status");
    if !RUNNABLE.contains(&state.as_str()) {
        return Err(StoreError::BadRequest(format!(
            "Workflow cannot be executed from {state} state"
        )));
    }

    let moment = stamp(payload);
    let mut steps = held
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut index = 0;
    for (at, step) in steps.iter_mut().enumerate() {
        let state = text(step, "status");
        // a step that already completed or was skipped is not taken twice
        if state == "completed" || state == "skipped" {
            continue;
        }
        step["status"] = json!("completed");
        step["started_at"] = moment.clone();
        step["completed_at"] = moment.clone();
        index = at;
    }

    Records::new(graph, WORKFLOW).update(
        scope,
        workflow_id,
        &json!({
            "status": "completed",
            "steps": steps,
            "current_step_index": index,
            "updated_at": moment,
        }),
    )?;

    Ok(json!({
        "workflow_id": workflow_id,
        "status": "completed",
        // the shipped service counted the whole step list, and that is what a reader has been comparing against
        "steps_completed": steps.len(),
        "requests_sent": 0,
        "note": "Steps are recorded as taken. This service does not call the services their addresses name.",
    }))
}

/// `POST /workflows/{id}/compensate`: roll back what ran, in reverse.
///
/// Only a step that completed AND carries a compensation address is rolled back. A step with nothing to undo stays
/// completed rather than being labelled compensated, because calling it compensated would claim an undo that never
/// happened.
pub fn compensate_orchestration(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    workflow_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let held = orchestration(graph, scope, workflow_id)?;
    let moment = stamp(payload);
    let mut steps = held
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut rolled_back = 0;
    for step in steps.iter_mut().rev() {
        if text(step, "status") == "completed" && !text(step, "compensation_url").is_empty() {
            step["status"] = json!("compensated");
            rolled_back += 1;
        }
    }

    Records::new(graph, WORKFLOW).update(
        scope,
        workflow_id,
        &json!({"status": "compensated", "steps": steps, "updated_at": moment}),
    )?;

    Ok(json!({
        "workflow_id": workflow_id,
        "status": "compensated",
        "steps_compensated": rolled_back,
        "requests_sent": 0,
        "note": "Steps are recorded as rolled back. This service does not call the compensation addresses.",
    }))
}

/// `DELETE /workflows/{id}`: close a workflow out.
///
/// Cancelling is unconditional, kept as shipped: it is how a caller closes a workflow out, and refusing it for some
/// states only leaves records that can never be closed.
pub fn cancel_orchestration(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    workflow_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    orchestration(graph, scope, workflow_id)?;
    Records::new(graph, WORKFLOW).update(
        scope,
        workflow_id,
        &json!({"status": "cancelled", "updated_at": stamp(payload)}),
    )?;
    Ok(json!({"workflow_id": workflow_id, "status": "cancelled"}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn talked() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn made(graph: &MemoryGraph) -> Value {
        create_orchestration(
            graph,
            &talked(),
            &json!({"name": "Month end across services", "now": 1000.0,
                    "steps": [
                        {"name": "Post accruals", "service_url": "http://accounting", "endpoint": "/journals",
                         "compensation_url": "http://accounting/journals/reverse"},
                        {"name": "Reconcile bank", "service_url": "http://banking", "endpoint": "/reconcile"},
                        {"name": "Produce report", "service_url": "http://reporting", "endpoint": "/reports"}
                    ]}),
        )
        .expect("a workflow")
    }

    #[test]
    fn a_workflow_starts_pending_with_every_step_defaulted() {
        let graph = MemoryGraph::new();
        let head = made(&graph);
        assert_eq!(head["status"], json!("pending"));
        assert_eq!(head["current_step_index"], json!(0));
        let steps = head["steps"].as_array().expect("steps");
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0]["method"], json!("POST"));
        assert_eq!(steps[0]["max_retries"], json!(3));
        assert_eq!(steps[0]["retry_count"], json!(0));
        assert_eq!(steps[0]["status"], json!("pending"));
        assert_eq!(steps[1]["payload"], json!({}));
        assert!(steps[0]["id"].as_str().is_some_and(|id| !id.is_empty()));

        // the listing narrows by state, and a workflow that is not there is missing
        let pending =
            orchestrations(&graph, &talked(), &json!({"status": "pending"})).expect("list");
        assert_eq!(pending.as_array().map(Vec::len), Some(1));
        let done =
            orchestrations(&graph, &talked(), &json!({"status": "completed"})).expect("list");
        assert_eq!(done.as_array().map(Vec::len), Some(0));
        assert!(orchestration(&graph, &talked(), "nope").is_err());
    }

    #[test]
    fn a_run_marks_the_steps_and_answers_honestly_about_what_it_sent() {
        let graph = MemoryGraph::new();
        let head = made(&graph);
        let id = text(&head, "id");

        let run =
            run_orchestration(&graph, &talked(), &id, &json!({"now": 2000.0})).expect("a run");
        assert_eq!(run["status"], json!("completed"));
        // the shipped service counted the whole step list, not just the steps it walked
        assert_eq!(run["steps_completed"], json!(3));
        // and it never called the services the steps name: the port says so instead of implying otherwise
        assert_eq!(run["requests_sent"], json!(0));

        let after = orchestration(&graph, &talked(), &id).expect("a workflow");
        assert_eq!(after["status"], json!("completed"));
        assert_eq!(after["current_step_index"], json!(2));
        assert_eq!(after["steps"][2]["status"], json!("completed"));
        assert_eq!(after["steps"][0]["completed_at"], json!(2000.0));

        // running a completed workflow again is a bad request, not a second run
        assert!(matches!(
            run_orchestration(&graph, &talked(), &id, &json!({})),
            Err(StoreError::BadRequest(_))
        ));

        // a failed workflow may be run again, which is how a caller retries
        Records::new(&graph, WORKFLOW)
            .update(&talked(), &id, &json!({"status": "failed"}))
            .expect("a change");
        assert!(run_orchestration(&graph, &talked(), &id, &json!({})).is_ok());
    }

    #[test]
    fn a_rollback_only_undoes_what_completed_and_can_be_undone() {
        let graph = MemoryGraph::new();
        let head = made(&graph);
        let id = text(&head, "id");
        run_orchestration(&graph, &talked(), &id, &json!({"now": 2000.0})).expect("a run");

        let rolled = compensate_orchestration(&graph, &talked(), &id, &json!({"now": 3000.0}))
            .expect("a rollback");
        assert_eq!(rolled["status"], json!("compensated"));
        // only the step that completed AND carries a compensation address is undone
        assert_eq!(rolled["steps_compensated"], json!(1));
        assert_eq!(rolled["requests_sent"], json!(0));

        let after = orchestration(&graph, &talked(), &id).expect("a workflow");
        assert_eq!(after["status"], json!("compensated"));
        assert_eq!(after["steps"][0]["status"], json!("compensated"));
        // the steps with nothing to undo stay completed rather than claiming an undo that never happened
        assert_eq!(after["steps"][1]["status"], json!("completed"));
        assert_eq!(after["steps"][2]["status"], json!("completed"));

        // cancelling closes the workflow out whatever state it reached
        let closed = cancel_orchestration(&graph, &talked(), &id, &json!({"now": 4000.0}))
            .expect("a cancel");
        assert_eq!(closed["status"], json!("cancelled"));
        let after = orchestration(&graph, &talked(), &id).expect("a workflow");
        assert_eq!(after["status"], json!("cancelled"));
        // a cancelled workflow cannot be run
        assert!(matches!(
            run_orchestration(&graph, &talked(), &id, &json!({})),
            Err(StoreError::BadRequest(_))
        ));
        assert!(cancel_orchestration(&graph, &talked(), "nope", &json!({})).is_err());

        // another Book sees and runs none of it
        let elsewhere = BookScope::new("user-2", "book-2");
        assert!(orchestration(&graph, &elsewhere, &id).is_err());
        assert!(run_orchestration(&graph, &elsewhere, &id, &json!({})).is_err());
        assert_eq!(
            orchestrations(&graph, &elsewhere, &json!({}))
                .expect("list")
                .as_array()
                .map(Vec::len),
            Some(0)
        );
    }
}
