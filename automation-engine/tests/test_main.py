"""Verification suite for automation-engine (triaged in-memory service, stays by design).

Covers: task CRUD lifecycle, enable/disable, manual run trigger, execution
history, running-execution listing, cancellation, task-types catalogue and
metrics aggregation. The engine's stores are module-level dicts by design
(stale/unregistered service); tests reset them between cases and inject
fake running executions where a deterministic RUNNING state is needed.
"""

import main
import pytest
from fastapi.testclient import TestClient
from main import TaskExecution, TaskStatus, TaskType

client = TestClient(app := main.app)


@pytest.fixture(autouse=True)
def _reset_stores():
    main.tasks.clear()
    main.executions.clear()
    main.running_tasks.clear()
    yield
    main.tasks.clear()
    main.executions.clear()
    main.running_tasks.clear()


def _create_task(name="Nightly ledger sweep", enabled=True):
    r = client.post(
        "/tasks",
        json={
            "name": name,
            "description": "Run the nightly ledger sweep",
            "task_type": "reconcile_accounts",
            "schedule": "0 2 * * *",
            "service_endpoint": "http://ledger-service/run",
            "payload": {"depth": "full"},
            "timeout_seconds": 120,
            "retry_count": 2,
            "enabled": enabled,
        },
    )
    assert r.status_code == 201, r.text
    return r.json()


def test_health_and_task_types():
    resp = client.get("/")
    assert resp.status_code == 200
    body = resp.json()
    assert body["status"] == "healthy"
    assert "automation-engine" in body["service"]

    types = client.get("/task-types").json()
    assert isinstance(types, list) and len(types) == len(list(TaskType))
    assert {"name", "value"} <= set(types[0].keys())


def test_task_crud_lifecycle():
    task = _create_task()
    assert task["id"]
    assert task["enabled"] is True
    assert task["schedule"] == "0 2 * * *"
    assert task["run_immediately"] is False  # default not exposed by create request

    # list + enabled_only filter
    assert len(client.get("/tasks").json()) == 1
    _create_task(name="Disabled one", enabled=False)
    assert len(client.get("/tasks").json()) == 2
    assert len(client.get("/tasks", params={"enabled_only": True}).json()) == 1

    # get by id, 404 on unknown
    got = client.get("/tasks/{}".format(task["id"])).json()
    assert got["name"] == "Nightly ledger sweep"
    assert client.get("/tasks/NOPE").status_code == 404

    # update keeps id, changes fields
    upd = client.put(
        "/tasks/{}".format(task["id"]),
        json={
            "name": "Hourly sweep",
            "task_type": "reconcile_accounts",
            "schedule": "0 * * * *",
            "service_endpoint": "http://ledger-service/run",
            "payload": {},
            "timeout_seconds": 60,
            "retry_count": 0,
            "enabled": False,
        },
    )
    assert upd.status_code == 200
    assert upd.json()["id"] == task["id"]
    assert upd.json()["schedule"] == "0 * * * *"
    assert upd.json()["enabled"] is False
    assert client.put("/tasks/NOPE", json={}).status_code == 422  # unknown id still validated

    # enable/disable round-trip
    assert client.post("/tasks/{}/disable".format(task["id"])).json() == {"status": "disabled"}
    assert client.post("/tasks/{}/enable".format(task["id"])).json() == {"status": "enabled"}
    assert client.post("/tasks/NOPE/enable").status_code == 404
    assert client.post("/tasks/NOPE/disable").status_code == 404

    # delete then 404s
    assert client.delete("/tasks/{}".format(task["id"])).json() == {"status": "deleted"}
    assert client.get("/tasks/{}".format(task["id"])).status_code == 404
    assert client.delete("/tasks/{}".format(task["id"])).status_code == 404


def test_manual_run_and_execution_history():
    task = _create_task()
    resp = client.post("/tasks/{}/run".format(task["id"]))
    assert resp.status_code == 200
    body = resp.json()
    assert body["status"] == "triggered"
    assert body["task_name"] == "Nightly ledger sweep"
    execution_id = body["execution_id"]

    # history records BOTH the pending stub from run_task_now and the actual
    # execution spawned by _execute_task (original engine behaviour, kept)
    hist = client.get("/tasks/{}/executions".format(task["id"])).json()
    assert len(hist) == 2
    assert {h["id"] for h in hist} >= {execution_id}
    assert {h["triggered_by"] for h in hist} == {"manual", "scheduler"}

    # unknown task -> empty history (original contract), unknown id for run -> 404
    assert client.get("/tasks/NOPE/executions").json() == []
    assert client.post("/tasks/NOPE/run").status_code == 404


def test_running_list_cancel_and_metrics():
    task = _create_task()

    # inject a deterministic RUNNING execution (avoid racing the 2s simulated work)
    fake = TaskExecution(
        id="exec-1",
        task_id=task["id"],
        task_name=task["name"],
        task_type=task["task_type"],
        status=TaskStatus.RUNNING,
        started_at=main.datetime.utcnow(),
        triggered_by="scheduler",
    )
    main.running_tasks[task["id"]] = fake
    main.executions[task["id"]] = [fake]

    running = client.get("/executions/running").json()
    assert len(running) == 1
    assert running[0]["id"] == "exec-1"
    assert running[0]["status"] == "running"

    # cancel flips status and clears the running list
    assert client.post("/executions/exec-1/cancel").json() == {"status": "cancelled"}
    assert client.get("/executions/running").json() == []
    hist = client.get("/tasks/{}/executions".format(task["id"])).json()
    assert hist[0]["status"] == "cancelled"
    assert hist[0]["completed_at"] is not None

    # cancelling an unknown/completed execution keeps the 404 contract
    assert client.post("/executions/exec-1/cancel").status_code == 404
    assert client.post("/executions/NOPE/cancel").status_code == 404

    # metrics reflect store state
    m = client.get("/metrics").json()
    assert m["total_tasks"] == 1
    assert m["enabled_tasks"] == 1
    assert m["running_tasks"] == 0
    assert m["total_executions"] == 1
    assert m["failed_executions"] == 0
    assert m["success_rate"] == 0.0  # cancelled is not a completed success


def test_execution_history_status_filter_and_limit():
    task = _create_task()
    now = main.datetime.utcnow()
    main.executions[task["id"]] = [
        TaskExecution(
            id="e-completed",
            task_id=task["id"],
            task_name=task["name"],
            task_type=task["task_type"],
            status=TaskStatus.COMPLETED,
            started_at=now,
            completed_at=now,
        ),
        TaskExecution(
            id="e-failed",
            task_id=task["id"],
            task_name=task["name"],
            task_type=task["task_type"],
            status=TaskStatus.FAILED,
            started_at=now,
        ),
    ]

    all_hist = client.get("/tasks/{}/executions".format(task["id"])).json()
    assert len(all_hist) == 2

    failed = client.get("/tasks/{}/executions".format(task["id"]), params={"status": "failed"}).json()
    assert len(failed) == 1
    assert failed[0]["id"] == "e-failed"

    limited = client.get("/tasks/{}/executions".format(task["id"]), params={"limit": 1}).json()
    assert len(limited) == 1

    m = client.get("/metrics").json()
    assert m["completed_executions"] == 1
    assert m["failed_executions"] == 1
    assert m["total_executions"] == 2
    assert m["success_rate"] == 0.5
