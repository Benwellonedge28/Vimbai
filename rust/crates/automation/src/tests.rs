//! Parity cases captured from automation-engine before it was retired.

use serde_json::json;

use crate::{Engine, EngineError, TaskStatus, TaskType};

fn request() -> serde_json::Value {
    json!({
        "name": "Nightly recon",
        "task_type": "reconcile_accounts",
        "schedule": "0 2 * * *",
        "service_endpoint": "/api/reconcile",
        "payload": {"a": 1}
    })
}

#[test]
fn a_fresh_engine_carries_the_seven_shipped_tasks() {
    let engine = Engine::with_defaults();
    let tasks = engine.list_tasks(false);
    assert_eq!(tasks.len(), 7);
    assert_eq!(tasks[0].id, "daily-reconciliation");
    assert_eq!(tasks[0].name, "Daily Account Reconciliation");
    assert_eq!(tasks[0].task_type, TaskType::ReconcileAccounts);
    assert_eq!(tasks[0].schedule, "0 2 * * *");
    assert_eq!(tasks[0].payload, json!({}));
    assert_eq!(tasks[0].timeout_seconds, 300);
    assert_eq!(tasks[0].retry_delay_seconds, 60);
    assert!(tasks[0].notification_on_failure);
    assert_eq!(tasks[0].max_concurrent, 1);
    // the dashboard refresher was the only short-timeout default
    let dashboard = engine.get_task("dashboard-refresh").unwrap();
    assert_eq!(dashboard.timeout_seconds, 120);
    assert_eq!(dashboard.schedule, "*/15 * * * *");
}

#[test]
fn create_fills_the_fields_the_wire_never_sent() {
    let mut engine = Engine::with_defaults();
    let task = engine.create_from_value("new-task", &request()).unwrap();
    assert_eq!(task.name, "Nightly recon");
    assert_eq!(task.description, None);
    assert!(task.enabled);
    assert_eq!(task.payload, json!({"a": 1}));
    assert_eq!(task.timeout_seconds, 300);
    assert_eq!(task.retry_count, 3);
    assert_eq!(task.retry_delay_seconds, 60);
    assert!(task.notification_on_failure);
    assert!(!task.run_immediately);
    assert_eq!(task.max_concurrent, 1);
    assert_eq!(engine.list_tasks(false).len(), 8);
}

#[test]
fn the_task_type_list_is_strict() {
    let mut engine = Engine::with_defaults();
    let mut bad = request();
    bad["task_type"] = json!("ledger_sweep");
    let err = engine.create_from_value("bad", &bad).unwrap_err();
    assert_eq!(
        err,
        EngineError::UnknownTaskType("ledger_sweep".to_string())
    );
    assert_eq!(err.status_code(), 422);
    assert_eq!(engine.list_tasks(false).len(), 7, "nothing was stored");
}

#[test]
fn disabling_hides_a_task_from_the_enabled_listing_only() {
    let mut engine = Engine::with_defaults();
    engine.create_from_value("new-task", &request()).unwrap();
    engine.set_enabled("new-task", false).unwrap();
    assert_eq!(engine.list_tasks(false).len(), 8);
    assert_eq!(engine.list_tasks(true).len(), 7);
    let created = engine.get_task("new-task").unwrap();
    assert!(!created.enabled);
    assert_eq!(
        engine.set_enabled("missing", true).unwrap_err(),
        EngineError::TaskNotFound
    );
}

#[test]
fn a_manual_run_writes_two_records() {
    let mut engine = Engine::with_defaults();
    engine.create_from_value("new-task", &request()).unwrap();
    let trigger = engine
        .trigger(
            "new-task",
            "manual",
            "exec-pending",
            "exec-started",
            "2026-11-03T08:07:17.956813",
        )
        .unwrap();
    assert_eq!(trigger.pending.status, TaskStatus::Pending);
    assert_eq!(trigger.pending.triggered_by, "manual");
    assert_eq!(trigger.pending.started_at, None);
    assert_eq!(trigger.started.status, TaskStatus::Running);
    assert_eq!(trigger.started.triggered_by, "scheduler");

    // the history keeps both, newest-startable first: the running record, then
    // the pending stub that never started
    let history = engine.executions_for("new-task", None, 50);
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].status, TaskStatus::Running);
    assert_eq!(history[1].status, TaskStatus::Pending);
    assert_eq!(engine.running().len(), 1);
    assert_eq!(engine.running()[0].id, "exec-started");
}

#[test]
fn the_pending_stub_counts_in_metrics_but_never_completes() {
    let mut engine = Engine::with_defaults();
    engine.create_from_value("new-task", &request()).unwrap();
    engine
        .trigger(
            "new-task",
            "manual",
            "exec-pending",
            "exec-started",
            "2026-11-03T08:07:17.956813",
        )
        .unwrap();
    let metrics = engine.metrics();
    assert_eq!(metrics.total_tasks, 8);
    assert_eq!(metrics.enabled_tasks, 8);
    assert_eq!(metrics.running_tasks, 1);
    assert_eq!(metrics.total_executions, 2);
    assert_eq!(metrics.completed_executions, 0);
    assert_eq!(metrics.success_rate, 0.0);

    engine
        .complete_execution(
            "exec-started",
            "2026-11-03T08:07:19.956813",
            2.0,
            json!({"success": true, "message": "Task completed successfully"}),
        )
        .unwrap();
    let after = engine.metrics();
    assert_eq!(after.running_tasks, 0);
    assert_eq!(after.completed_executions, 1);
    assert_eq!(after.success_rate, 0.5); // one stub will never finish
    assert_eq!(
        after.to_json(),
        json!({
            "total_tasks": 8,
            "enabled_tasks": 8,
            "running_tasks": 0,
            "total_executions": 2,
            "completed_executions": 1,
            "failed_executions": 0,
            "success_rate": 0.5,
        })
    );
}

#[test]
fn history_filters_by_status_and_honours_the_limit() {
    let mut engine = Engine::with_defaults();
    engine.create_from_value("new-task", &request()).unwrap();
    for (n, minute) in [(1, "01"), (2, "02"), (3, "03")] {
        engine
            .trigger(
                "new-task",
                "manual",
                format!("pending-{n}"),
                format!("started-{n}"),
                format!("2026-11-03T08:{minute}:00"),
            )
            .unwrap();
        let closed = format!("started-{n}");
        engine
            .complete_execution(
                &closed,
                format!("2026-11-03T08:{minute}:02"),
                2.0,
                json!({"success": true}),
            )
            .unwrap();
    }
    // every run was closed as it happened, so nothing is still running
    assert_eq!(engine.running().len(), 0);
    let all = engine.executions_for("new-task", None, 50);
    assert_eq!(all.len(), 6, "three runs, two records each");
    assert_eq!(
        engine
            .executions_for("new-task", Some(TaskStatus::Completed), 50)
            .len(),
        3
    );
    assert_eq!(
        engine
            .executions_for("new-task", Some(TaskStatus::Pending), 50)
            .len(),
        3
    );
    assert_eq!(engine.executions_for("new-task", None, 1).len(), 1);
    assert_eq!(engine.executions_for("never-ran", None, 50).len(), 0);
}

#[test]
fn cancelling_uses_the_running_set_and_404s_otherwise() {
    let mut engine = Engine::with_defaults();
    engine.create_from_value("new-task", &request()).unwrap();
    engine
        .trigger(
            "new-task",
            "manual",
            "exec-pending",
            "exec-started",
            "2026-11-03T08:07:17",
        )
        .unwrap();
    let cancelled = engine
        .cancel_execution_at("exec-started", "2026-11-03T08:07:18")
        .unwrap();
    assert_eq!(cancelled.status, TaskStatus::Cancelled);
    assert_eq!(
        cancelled.completed_at.as_deref(),
        Some("2026-11-03T08:07:18")
    );
    assert!(engine.running().is_empty());

    let err = engine.cancel_execution("exec-started").unwrap_err();
    assert_eq!(err, EngineError::ExecutionNotFound);
    assert_eq!(err.status_code(), 404);
    assert_eq!(err.message(), "Execution not found or already completed");

    // the pending stub was never running, so it cannot be cancelled either
    assert!(engine.cancel_execution("exec-pending").is_err());
}

#[test]
fn a_failed_run_records_its_error() {
    let mut engine = Engine::with_defaults();
    engine.create_from_value("new-task", &request()).unwrap();
    engine
        .trigger("new-task", "manual", "p", "s", "2026-11-03T08:07:17")
        .unwrap();
    let failed = engine
        .fail_execution("s", "2026-11-03T08:07:18", 1.0, "upstream timeout")
        .unwrap();
    assert_eq!(failed.status, TaskStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("upstream timeout"));
    assert_eq!(engine.metrics().failed_executions, 1);
}

#[test]
fn update_replaces_the_request_fields_and_keeps_the_hidden_ones() {
    let mut engine = Engine::with_defaults();
    let created = engine.create_from_value("new-task", &request()).unwrap();
    assert_eq!(created.description, None);
    let mut body = request();
    body["name"] = json!("Renamed recon");
    body["description"] = json!("now with words");
    body["enabled"] = json!(false);
    let request = crate::TaskRequest::from_value(&body).unwrap();
    let updated = engine.update_task("new-task", &request).unwrap();
    assert_eq!(updated.id, "new-task");
    assert_eq!(updated.name, "Renamed recon");
    assert_eq!(updated.description.as_deref(), Some("now with words"));
    assert!(!updated.enabled);
    assert_eq!(updated.retry_delay_seconds, 60, "not exposed on the wire");
    assert_eq!(
        engine.update_task("missing", &request).unwrap_err(),
        EngineError::TaskNotFound
    );
}

#[test]
fn deleting_keeps_the_history_and_the_second_delete_is_a_404() {
    let mut engine = Engine::with_defaults();
    engine.create_from_value("new-task", &request()).unwrap();
    engine
        .trigger("new-task", "manual", "p", "s", "2026-11-03T08:07:17")
        .unwrap();
    engine.delete_task("new-task").unwrap();
    assert_eq!(engine.executions_for("new-task", None, 50).len(), 2);
    assert!(engine.get_task("new-task").is_err());
    let err = engine.delete_task("new-task").unwrap_err();
    assert_eq!(err, EngineError::TaskNotFound);
    assert_eq!(err.message(), "Task not found");
}

#[test]
fn task_types_are_published_in_enum_order() {
    let catalogue = Engine::task_types();
    assert_eq!(catalogue.len(), 25);
    assert_eq!(
        catalogue[0],
        json!({"name": "RECONCILE_ACCOUNTS", "value": "reconcile_accounts"})
    );
    assert_eq!(
        catalogue[24],
        json!({"name": "SYNC_OFFLINE_DATA", "value": "sync_offline_data"})
    );
    // the list the service advertised was exactly the parseable set
    for entry in catalogue {
        let value = entry["value"].as_str().unwrap();
        assert!(TaskType::parse(value).is_ok(), "{value} must parse");
    }
}
