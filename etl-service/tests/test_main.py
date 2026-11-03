"""ETL pipelines: Book-scoped Neo4j persistence.

Pipelines used to sit in a module-level list, so every restart lost them and
every tenant shared them. These tests pin the persisted, caller-scoped contract.
"""

import etl_store as store
import pytest
from fake_neo4j import FakeDriver, FakeSession
from fastapi.testclient import TestClient
from main import app

BOOK_ONE = {"X-User-Id": "user-1", "X-Book-ID": "book-1"}
BOOK_TWO = {"X-User-Id": "user-2", "X-Book-ID": "book-2"}

STEPS = [{"name": "pull", "step_type": "extract", "source": "ledger", "target": "staging"}]


@pytest.fixture
def client():
    store.set_driver(FakeDriver(FakeSession()))
    yield TestClient(app)
    store.set_driver(None)


def _create(client, name="nightly", headers=BOOK_ONE):
    resp = client.post(
        "/pipelines",
        json={"name": name, "description": "d", "steps": STEPS, "schedule": "0 2 * * *"},
        headers=headers,
    )
    assert resp.status_code == 200, resp.text
    return resp.json()


def test_health(client):
    assert client.get("/health").json()["service"] == "etl-service"


def test_pipeline_round_trips_with_steps(client):
    created = _create(client)
    assert created["steps"][0]["name"] == "pull"
    fetched = client.get(f"/pipelines/{created['id']}", headers=BOOK_ONE)
    assert fetched.status_code == 200
    assert fetched.json()["steps"][0]["step_type"] == "extract"
    assert client.get("/pipelines", headers=BOOK_ONE).json()[0]["id"] == created["id"]


def test_run_persists_the_completed_status(client):
    created = _create(client)
    run = client.post(f"/pipelines/{created['id']}/run", headers=BOOK_ONE)
    assert run.status_code == 200
    assert run.json()["steps_completed"] == 1

    after = client.get(f"/pipelines/{created['id']}", headers=BOOK_ONE).json()
    assert after["status"] == "completed"
    assert after["last_run"] is not None
    assert after["steps"][0]["status"] == "completed"


def test_delete_removes_the_pipeline(client):
    created = _create(client)
    assert client.delete(f"/pipelines/{created['id']}", headers=BOOK_ONE).json()["deleted"] is True
    assert client.get(f"/pipelines/{created['id']}", headers=BOOK_ONE).status_code == 404


def test_one_book_never_sees_another_books_pipelines(client):
    mine = _create(client, "mine", BOOK_ONE)
    assert client.get("/pipelines", headers=BOOK_TWO).json() == []
    assert client.get(f"/pipelines/{mine['id']}", headers=BOOK_TWO).status_code == 404
    assert client.post(f"/pipelines/{mine['id']}/run", headers=BOOK_TWO).status_code == 404
    assert client.delete(f"/pipelines/{mine['id']}", headers=BOOK_TWO).status_code == 404


def test_status_filter(client):
    _create(client, "one")
    done = _create(client, "two")
    client.post(f"/pipelines/{done['id']}/run", headers=BOOK_ONE)
    completed = client.get("/pipelines", params={"status": "completed"}, headers=BOOK_ONE).json()
    assert [p["id"] for p in completed] == [done["id"]]
