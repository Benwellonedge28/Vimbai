#![forbid(unsafe_code)]
//! workflow-service: the definitions a step runs through and the instances that walk them.
//!
//! Behaviours kept as shipped, each pinned by a test:
//!
//! * a definition holds its steps in order, and each step names the steps that come after it and, optionally, the ones
//!   a rejection goes to instead. It is active unless it is not, and a definition that is not active cannot be started;
//! * an instance starts at the FIRST step in the definition's list - the shipped engine took the first step rather than
//!   a step named as the start - and refuses a definition with no steps at all. The first step's role, person and
//!   configuration become the first task, which arrives pending, and the instance is running;
//! * a task completes with the action it was given (approve, reject or complete), who completed it, when and any
//!   comment; the task leaves the instance's current steps and the steps that follow arrive as new pending tasks;
//! * a rejection follows the rejection route ONLY when the step named one. An empty rejection route is no route, so a
//!   rejection with nothing listed falls through to the ordinary next steps - which is what the shipped truthiness test
//!   did, and is pinned here rather than tidied away;
//! * a task that is not waiting cannot be completed again, and the refusal names the state it is already in;
//! * an instance is complete once nothing is current, and its end is stamped. An instance that was failed or cancelled
//!   stays that way even when further tasks appear;
//! * instances and definitions are the caller's own: another Book's is missing rather than refused.
//!
//! The clock is taken from the request, so a workflow can be reasoned about and tested exactly.

use serde_json::{json, Value};

use vimbai_store::read::{field, flag, optional, refused, text};
use vimbai_store::{BookScope, Graph, RecordKind, Records, StoreError};

/// A set of steps and the routes between them.
pub const DEFINITION: RecordKind =
    RecordKind::new("WorkflowDefinition", "OWNS_WORKFLOW_DEFINITION");

/// One walk through a definition.
pub const INSTANCE: RecordKind = RecordKind::new("WorkflowInstance", "OWNS_WORKFLOW_INSTANCE");

/// The steps a definition holds, in order.
fn steps_of(record: &Value) -> Vec<Value> {
    record
        .get("steps")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The tasks an instance holds, in the order they were raised.
fn tasks_of(record: &Value) -> Vec<Value> {
    record
        .get("tasks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The steps an instance is currently waiting on.
fn current_of(record: &Value) -> Vec<String> {
    record
        .get("current_step_ids")
        .and_then(Value::as_array)
        .map(|held| {
            held.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The step identifiers a step names as coming next, given as a list or a single name.
fn named(step: &Value, key: &str) -> Vec<String> {
    match step.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(one)) if !one.is_empty() => vec![one.clone()],
        _ => Vec::new(),
    }
}

fn definition_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "name": text(record, "name"),
        "description": text(record, "description"),
        "steps": steps_of(record),
        "is_active": flag(record, "is_active"),
        "created_at": optional(record, "created_at"),
        "updated_at": optional(record, "updated_at"),
    })
}

fn instance_answer(record: &Value) -> Value {
    json!({
        "id": field(record, "id"),
        "workflow_definition_id": text(record, "workflow_definition_id"),
        "status": text(record, "status"),
        "current_step_ids": current_of(record),
        "tasks": tasks_of(record),
        "context": record.get("context").cloned().unwrap_or(Value::Null),
        "triggered_by_event": record.get("triggered_by_event").cloned().unwrap_or(Value::Null),
        "start_date": optional(record, "start_date"),
        "end_date": optional(record, "end_date"),
    })
}

/// `POST /workflow-definitions`: hold a definition.
pub fn create_definition(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let name = text(payload, "name");
    if name.is_empty() {
        return Err(refused("name is required"));
    }
    let held = Records::new(graph, DEFINITION).create(
        scope,
        &json!({
            "id": vimbai_common::ids::service_id(),
            "name": name,
            "description": text(payload, "description"),
            "steps": steps_of(payload),
            "is_active": match payload.get("is_active") {
                None | Some(Value::Null) => true,
                Some(_) => flag(payload, "is_active"),
            },
            "created_at": text(payload, "created_at"),
            "updated_at": text(payload, "created_at"),
        }),
    )?;
    Ok(definition_answer(&held))
}

/// `GET /workflow-definitions`: the caller's definitions.
pub fn definitions(graph: &(impl Graph + ?Sized), scope: &BookScope) -> Result<Value, StoreError> {
    let held = Records::new(graph, DEFINITION).list(scope, 1000)?;
    Ok(Value::Array(held.iter().map(definition_answer).collect()))
}

/// `GET /workflow-definitions/{id}`: one definition.
pub fn definition_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    definition_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, DEFINITION).get(scope, definition_id)?;
    Ok(definition_answer(&held))
}

/// `PUT /workflow-definitions/{id}`: change what was given, leave the rest.
pub fn update_definition(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    definition_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, DEFINITION);
    records.get(scope, definition_id)?;
    let mut change = json!({"updated_at": text(payload, "updated_at")});
    for (name, given) in payload.as_object().cloned().unwrap_or_default() {
        match name.as_str() {
            "name" => change["name"] = json!(given),
            "description" => change["description"] = json!(given),
            "steps" => change["steps"] = given,
            "is_active" => change["is_active"] = json!(flag(payload, "is_active")),
            _ => {}
        }
    }
    records.update(scope, definition_id, &change)?;
    definition_of(graph, scope, definition_id)
}

/// `DELETE /workflow-definitions/{id}`: drop a definition.
pub fn delete_definition(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    definition_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, DEFINITION).delete(scope, definition_id)?;
    Ok(json!({"ok": true}))
}

/// A task raised from a step: pending, with the step's role, person and configuration.
fn task_from(step: &Value, when: &str) -> Value {
    json!({
        "task_id": vimbai_common::ids::service_id(),
        "step_id": text(step, "step_id"),
        "status": "pending",
        "assigned_to_role": optional(step, "assignee_role"),
        "assigned_to_user_id": optional(step, "assignee_user_id"),
        "payload": step.get("config").cloned().unwrap_or_else(|| json!({})),
        "completed_by_user_id": Value::Null,
        "completed_at": Value::Null,
        "comments": Value::Null,
        "raised_at": when,
    })
}

/// `POST /workflow-definitions/{id}/start`: begin an instance at the definition's first step.
pub fn start_instance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    definition_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let definition = Records::new(graph, DEFINITION).get(scope, definition_id)?;
    // a definition that is switched off cannot be started
    if !flag(&definition, "is_active") {
        return Err(refused(&format!(
            "workflow definition {definition_id} is not active"
        )));
    }
    let steps = steps_of(&definition);
    // the shipped engine took the first step in the list as the way in
    let Some(first) = steps.first() else {
        return Err(refused(&format!(
            "workflow definition {definition_id} has no steps defined"
        )));
    };
    let when = text(payload, "now");
    let task = task_from(first, &when);
    let instance = Records::new(graph, INSTANCE).create(
        scope,
        &json!({
            "id": vimbai_common::ids::service_id(),
            "workflow_definition_id": definition_id,
            "triggered_by_event": payload.get("triggered_by_event").cloned().unwrap_or(Value::Null),
            "context": payload.get("context").cloned().unwrap_or_else(|| json!({})),
            "status": "running",
            "tasks": [task.clone()],
            "current_step_ids": [text(&task, "task_id")],
            "start_date": when,
            "end_date": Value::Null,
        }),
    )?;
    Ok(instance_answer(&instance))
}

/// `POST /workflow-instances`: hold an instance exactly as it was given.
pub fn create_instance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    payload: &Value,
) -> Result<Value, StoreError> {
    let definition_id = text(payload, "workflow_definition_id");
    if definition_id.is_empty() {
        return Err(refused("workflow_definition_id is required"));
    }
    let held = Records::new(graph, INSTANCE).create(
        scope,
        &json!({
            "id": vimbai_common::ids::service_id(),
            "workflow_definition_id": definition_id,
            "triggered_by_event": payload.get("triggered_by_event").cloned().unwrap_or(Value::Null),
            "context": payload.get("context").cloned().unwrap_or_else(|| json!({})),
            "status": match payload.get("status") {
                Some(Value::String(given)) if !given.is_empty() => given.clone(),
                _ => "running".to_string(),
            },
            "tasks": tasks_of(payload),
            "current_step_ids": current_of(payload),
            "start_date": text(payload, "now"),
            "end_date": Value::Null,
        }),
    )?;
    Ok(instance_answer(&held))
}

/// `GET /workflow-instances/{id}`: one instance.
pub fn instance_of(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    instance_id: &str,
) -> Result<Value, StoreError> {
    let held = Records::new(graph, INSTANCE).get(scope, instance_id)?;
    Ok(instance_answer(&held))
}

/// `PUT /workflow-instances/{id}`: change what was given, leave the rest.
pub fn update_instance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    instance_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let records = Records::new(graph, INSTANCE);
    records.get(scope, instance_id)?;
    let mut change = json!({});
    for (name, given) in payload.as_object().cloned().unwrap_or_default() {
        match name.as_str() {
            "status" => change["status"] = given,
            "context" => change["context"] = given,
            "tasks" => change["tasks"] = given,
            "current_step_ids" => change["current_step_ids"] = given,
            "end_date" => change["end_date"] = given,
            _ => {}
        }
    }
    records.update(scope, instance_id, &change)?;
    instance_of(graph, scope, instance_id)
}

/// `DELETE /workflow-instances/{id}`: drop an instance.
pub fn delete_instance(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    instance_id: &str,
) -> Result<Value, StoreError> {
    Records::new(graph, INSTANCE).delete(scope, instance_id)?;
    Ok(json!({"ok": true}))
}

/// `POST /workflow-instances/{id}/complete`: finish a waiting task and raise whatever follows it.
pub fn complete_task(
    graph: &(impl Graph + ?Sized),
    scope: &BookScope,
    instance_id: &str,
    payload: &Value,
) -> Result<Value, StoreError> {
    let instances = Records::new(graph, INSTANCE);
    let instance = instances.get(scope, instance_id)?;
    let definition_id = text(&instance, "workflow_definition_id");
    let definition = Records::new(graph, DEFINITION).get(scope, &definition_id)?;
    let steps = steps_of(&definition);
    let action = text(payload, "action");
    let task_id = text(payload, "task_id");

    let mut tasks = tasks_of(&instance);
    let Some(position) = tasks
        .iter()
        .position(|task| text(task, "task_id") == task_id)
    else {
        return Err(StoreError::NotFound);
    };
    let waiting = text(&tasks[position], "status");
    // a task that is not waiting cannot be completed again, and the refusal names where it stands
    if waiting != "pending" && waiting != "in_progress" {
        return Err(refused(&format!(
            "workflow task {task_id} is already {waiting}"
        )));
    }
    let when = text(payload, "now");
    tasks[position]["status"] = json!(action);
    tasks[position]["completed_by_user_id"] = payload
        .get("completed_by_user_id")
        .cloned()
        .unwrap_or(Value::Null);
    tasks[position]["completed_at"] = json!(when);
    tasks[position]["comments"] = payload.get("comments").cloned().unwrap_or(Value::Null);

    let completed_step = text(&tasks[position], "step_id");
    let mut current: Vec<String> = current_of(&instance)
        .into_iter()
        .filter(|held| held != &task_id)
        .collect();

    // the shipped engine followed the rejection route only when a step named one; an empty route fell through
    let following: Vec<String> = steps
        .iter()
        .find(|step| text(step, "step_id") == completed_step)
        .map(|step| {
            let rejection = named(step, "on_rejection_steps");
            if action == "reject" && !rejection.is_empty() {
                rejection
            } else {
                named(step, "next_steps")
            }
        })
        .unwrap_or_default();

    for next in &following {
        let Some(step) = steps.iter().find(|step| &text(step, "step_id") == next) else {
            // a route that names a step the definition does not hold goes nowhere, as shipped
            continue;
        };
        let raised = task_from(step, &when);
        current.push(text(&raised, "task_id"));
        tasks.push(raised);
    }

    let before = text(&instance, "status");
    // a finished instance is complete; a failed or cancelled one keeps its state rather than being revived
    let status = if current.is_empty() {
        "completed".to_string()
    } else if before != "failed" && before != "cancelled" {
        "running".to_string()
    } else {
        before
    };
    instances.update(
        scope,
        instance_id,
        &json!({
            "status": status,
            "tasks": tasks,
            "current_step_ids": current,
            "end_date": if current.is_empty() { json!(when) } else { Value::Null },
        }),
    )?;
    instance_of(graph, scope, instance_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vimbai_store::fake::MemoryGraph;

    fn mine() -> BookScope {
        BookScope::new("user-1", "book-1")
    }

    fn two_step(graph: &MemoryGraph) -> String {
        let made = create_definition(
            graph,
            &mine(),
            &json!({
                "name": "Purchase approval",
                "steps": [
                    {"step_id": "check", "assignee_role": "clerk", "config": {"limit": 500},
                     "next_steps": ["sign"], "on_rejection_steps": []},
                    {"step_id": "sign", "assignee_role": "director", "next_steps": []}
                ],
                "created_at": "2026-01-01T09:00:00Z"
            }),
        )
        .expect("a definition");
        made["id"].as_str().expect("an id").to_string()
    }

    #[test]
    fn a_definition_holds_its_steps_and_is_active_unless_switched_off() {
        let graph = MemoryGraph::new();
        let definition_id = two_step(&graph);
        let held = definition_of(&graph, &mine(), &definition_id).expect("a definition");
        assert_eq!(held["is_active"], json!(true));
        assert_eq!(held["steps"].as_array().map(|steps| steps.len()), Some(2));
        assert_eq!(
            definitions(&graph, &mine())
                .expect("definitions")
                .as_array()
                .map(|held| held.len()),
            Some(1)
        );

        let switched = update_definition(
            &graph,
            &mine(),
            &definition_id,
            &json!({"is_active": false, "updated_at": "2026-01-02T09:00:00Z"}),
        )
        .expect("an update");
        assert_eq!(switched["is_active"], json!(false));
        // the steps were not touched by a change that did not mention them
        assert_eq!(
            switched["steps"].as_array().map(|steps| steps.len()),
            Some(2)
        );
        assert!(create_definition(&graph, &mine(), &json!({})).is_err());
    }

    #[test]
    fn an_instance_starts_at_the_first_step_in_the_list() {
        let graph = MemoryGraph::new();
        let definition_id = two_step(&graph);
        let started = start_instance(
            &graph,
            &mine(),
            &definition_id,
            &json!({"now": "2026-01-02T09:00:00Z", "context": {"invoice": "INV-1"}}),
        )
        .expect("an instance");
        assert_eq!(started["status"], json!("running"));
        let tasks = started["tasks"].as_array().expect("tasks");
        assert_eq!(tasks.len(), 1);
        // the first step in the list, with its role and its configuration as the payload
        assert_eq!(tasks[0]["step_id"], json!("check"));
        assert_eq!(tasks[0]["assigned_to_role"], json!("clerk"));
        assert_eq!(tasks[0]["status"], json!("pending"));
        assert_eq!(tasks[0]["payload"], json!({"limit": 500}));
        assert_eq!(
            started["current_step_ids"]
                .as_array()
                .map(|held| held.len()),
            Some(1)
        );
        // a switched-off definition cannot be started
        update_definition(
            &graph,
            &mine(),
            &definition_id,
            &json!({"is_active": false, "updated_at": "2026-01-02T09:00:00Z"}),
        )
        .expect("an update");
        assert!(start_instance(&graph, &mine(), &definition_id, &json!({"now": "x"})).is_err());
    }

    #[test]
    fn completing_a_task_raises_the_steps_that_follow_it() {
        let graph = MemoryGraph::new();
        let definition_id = two_step(&graph);
        let started = start_instance(
            &graph,
            &mine(),
            &definition_id,
            &json!({"now": "2026-01-02T09:00:00Z"}),
        )
        .expect("an instance");
        let instance_id = started["id"].as_str().expect("an id").to_string();
        let first_task = started["tasks"][0]["task_id"]
            .as_str()
            .expect("a task")
            .to_string();

        let after = complete_task(
            &graph,
            &mine(),
            &instance_id,
            &json!({"task_id": first_task, "action": "approve", "completed_by_user_id": "user-2",
                    "comments": "checked", "now": "2026-01-02T10:00:00Z"}),
        )
        .expect("a completion");
        assert_eq!(after["status"], json!("running"));
        let tasks = after["tasks"].as_array().expect("tasks");
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0]["status"], json!("approve"));
        assert_eq!(tasks[0]["completed_by_user_id"], json!("user-2"));
        assert_eq!(tasks[0]["comments"], json!("checked"));
        assert_eq!(tasks[1]["step_id"], json!("sign"));
        // only the step waiting now is current
        assert_eq!(
            after["current_step_ids"].as_array().map(|held| held.len()),
            Some(1)
        );
        assert_eq!(after["current_step_ids"][0], tasks[1]["task_id"]);

        // the last step ends the instance
        let second_task = tasks[1]["task_id"].as_str().expect("a task").to_string();
        let done = complete_task(
            &graph,
            &mine(),
            &instance_id,
            &json!({"task_id": second_task, "action": "approve", "now": "2026-01-02T11:00:00Z"}),
        )
        .expect("a completion");
        assert_eq!(done["status"], json!("completed"));
        assert_eq!(done["end_date"], json!("2026-01-02T11:00:00Z"));
        assert_eq!(
            done["current_step_ids"].as_array().map(|held| held.len()),
            Some(0)
        );

        // and the task cannot be completed twice
        assert!(complete_task(
            &graph,
            &mine(),
            &instance_id,
            &json!({"task_id": second_task, "action": "approve", "now": "2026-01-02T12:00:00Z"})
        )
        .is_err());
    }

    #[test]
    fn a_rejection_follows_its_own_route_only_when_the_step_named_one() {
        let graph = MemoryGraph::new();
        // this step names a rejection route
        let routed = create_definition(
            &graph,
            &mine(),
            &json!({
                "name": "With a rejection route",
                "steps": [
                    {"step_id": "check", "next_steps": ["sign"], "on_rejection_steps": ["send-back"]},
                    {"step_id": "sign", "next_steps": []},
                    {"step_id": "send-back", "next_steps": []}
                ],
                "created_at": "2026-01-01T09:00:00Z"
            }),
        )
        .expect("a definition");
        let route = routed["id"].as_str().expect("an id").to_string();
        let started = start_instance(
            &graph,
            &mine(),
            &route,
            &json!({"now": "2026-01-02T09:00:00Z"}),
        )
        .expect("an instance");
        let instance_id = started["id"].as_str().expect("an id").to_string();
        let task_id = started["tasks"][0]["task_id"]
            .as_str()
            .expect("a task")
            .to_string();
        let rejected = complete_task(
            &graph,
            &mine(),
            &instance_id,
            &json!({"task_id": task_id, "action": "reject", "now": "2026-01-02T10:00:00Z"}),
        )
        .expect("a completion");
        assert_eq!(rejected["tasks"][1]["step_id"], json!("send-back"));

        // an empty rejection route is no route, so a rejection falls through to the ordinary next step
        let graph = MemoryGraph::new();
        let plain = two_step(&graph);
        let started = start_instance(
            &graph,
            &mine(),
            &plain,
            &json!({"now": "2026-01-02T09:00:00Z"}),
        )
        .expect("an instance");
        let instance_id = started["id"].as_str().expect("an id").to_string();
        let task_id = started["tasks"][0]["task_id"]
            .as_str()
            .expect("a task")
            .to_string();
        let rejected = complete_task(
            &graph,
            &mine(),
            &instance_id,
            &json!({"task_id": task_id, "action": "reject", "now": "2026-01-02T10:00:00Z"}),
        )
        .expect("a completion");
        assert_eq!(rejected["tasks"][1]["step_id"], json!("sign"));
    }

    #[test]
    fn an_instance_that_was_failed_stays_failed() {
        let graph = MemoryGraph::new();
        let definition_id = two_step(&graph);
        let started = start_instance(
            &graph,
            &mine(),
            &definition_id,
            &json!({"now": "2026-01-02T09:00:00Z"}),
        )
        .expect("an instance");
        let instance_id = started["id"].as_str().expect("an id").to_string();
        let task_id = started["tasks"][0]["task_id"]
            .as_str()
            .expect("a task")
            .to_string();
        update_instance(&graph, &mine(), &instance_id, &json!({"status": "failed"}))
            .expect("an update");
        let after = complete_task(
            &graph,
            &mine(),
            &instance_id,
            &json!({"task_id": task_id, "action": "approve", "now": "2026-01-02T10:00:00Z"}),
        )
        .expect("a completion");
        // further work does not quietly revive it
        assert_eq!(after["status"], json!("failed"));
    }

    #[test]
    fn another_books_work_is_missing_rather_than_refused() {
        let graph = MemoryGraph::new();
        let definition_id = two_step(&graph);
        let theirs = BookScope::new("user-2", "book-2");
        assert!(definition_of(&graph, &theirs, &definition_id).is_err());
        assert_eq!(
            definitions(&graph, &theirs)
                .expect("definitions")
                .as_array()
                .map(|held| held.len()),
            Some(0)
        );
        assert!(start_instance(&graph, &theirs, &definition_id, &json!({"now": "x"})).is_err());
        assert!(delete_definition(&graph, &theirs, &definition_id).is_err());
    }
}
