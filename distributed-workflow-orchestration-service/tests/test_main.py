"""Distributed workflows: Book-scoped Neo4j persistence.

Workflows were a module-level list, so a restart lost every in-flight
orchestration and one tenant could execute or cancel another's workflow.
"""

import pytest
import workflow_store as store
from fake_neo4j import FakeDriver, FakeSession
from fastapi.testclient import TestClient
from main import app

BOOK_ONE = {"X-User-Id": "user-1", "X-Book-ID": "book-1"}
BOOK_TWO = {"X-User-Id": "user-2", "X-Book-ID": "book-2"}

STEPS = [
    {
        "name": "reserve",
        "service_url": "http://inventory:8000",
        "endpoint": "/reserve",
        "compensation_url": "http://inventory:8000/release",
    }
]


@pytest.fixture
def client():
    store.set_driver(FakeDriver(FakeSession()))
    yield TestClient(app)
    store.set_driver(None)


def _create(client, name="close-period", headers=BOOK_ONE):
    resp = client.post("/workflows", json={"name": name, "description": "d", "steps": STEPS}, headers=headers)
    assert resp.status_code == 200, resp.text
    return resp.json()


def test_health(client):
    assert client.get("/health").json()["service"] == "distributed-workflow-orchestration-service"


def test_workflow_round_trips(client):
    created = _create(client)
    assert created["status"] == "pending"
    assert created["steps"][0]["name"] == "reserve"
    assert client.get(f"/workflows/{created['id']}", headers=BOOK_ONE).json()["steps"][0]["name"] == "reserve"


def test_execute_persists_completion(client):
    created = _create(client)
    run = client.post(f"/workflows/{created['id']}/execute", headers=BOOK_ONE)
    assert run.status_code == 200, run.text
    after = client.get(f"/workflows/{created['id']}", headers=BOOK_ONE).json()
    assert after["status"] == "completed"
    assert after["steps"][0]["status"] == "completed"
    assert after["current_step_index"] == 0


def test_execute_is_refused_from_a_terminal_state(client):
    created = _create(client)
    client.post(f"/workflows/{created['id']}/execute", headers=BOOK_ONE)
    again = client.post(f"/workflows/{created['id']}/execute", headers=BOOK_ONE)
    assert again.status_code == 400


def test_compensate_marks_completed_steps(client):
    created = _create(client)
    client.post(f"/workflows/{created['id']}/execute", headers=BOOK_ONE)
    comp = client.post(f"/workflows/{created['id']}/compensate", headers=BOOK_ONE)
    assert comp.status_code == 200
    after = client.get(f"/workflows/{created['id']}", headers=BOOK_ONE).json()
    assert after["status"] == "compensated"
    assert after["steps"][0]["status"] == "compensated"


def test_cancel_sets_status_and_keeps_the_record(client):
    created = _create(client)
    assert client.delete(f"/workflows/{created['id']}", headers=BOOK_ONE).json()["status"] == "cancelled"
    assert client.get(f"/workflows/{created['id']}", headers=BOOK_ONE).json()["status"] == "cancelled"


def test_one_book_can_neither_see_nor_drive_another_books_workflow(client):
    mine = _create(client, headers=BOOK_ONE)
    assert client.get("/workflows", headers=BOOK_TWO).json() == []
    assert client.get(f"/workflows/{mine['id']}", headers=BOOK_TWO).status_code == 404
    assert client.post(f"/workflows/{mine['id']}/execute", headers=BOOK_TWO).status_code == 404
    assert client.post(f"/workflows/{mine['id']}/compensate", headers=BOOK_TWO).status_code == 404
    assert client.delete(f"/workflows/{mine['id']}", headers=BOOK_TWO).status_code == 404
    assert client.get("/workflows", headers=BOOK_ONE).json()[0]["status"] == "pending"
