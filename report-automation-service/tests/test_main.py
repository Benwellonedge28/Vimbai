"""Report automation: Book-scoped Neo4j persistence.

Templates, schedules and generated reports were module-level lists, so a
restart lost every schedule and one tenant could read another's reports.
"""

import pytest
import report_automation_store as store
from fake_neo4j import FakeDriver, FakeSession
from fastapi.testclient import TestClient
from main import app

BOOK_ONE = {"X-User-Id": "user-1", "X-Book-ID": "book-1"}
BOOK_TWO = {"X-User-Id": "user-2", "X-Book-ID": "book-2"}


@pytest.fixture
def client():
    store.set_driver(FakeDriver(FakeSession()))
    yield TestClient(app)
    store.set_driver(None)


def _template(client, headers=BOOK_ONE, name="Board pack"):
    resp = client.post(
        "/templates",
        params={"name": name, "report_type": "financial_summary", "description": "d"},
        headers=headers,
    )
    assert resp.status_code == 200, resp.text
    return resp.json()


def _schedule(client, template_id, headers=BOOK_ONE):
    resp = client.post(
        "/schedules",
        params={"template_id": template_id, "name": "monthly", "cron_expression": "0 0 1 * *"},
        headers=headers,
    )
    assert resp.status_code == 200, resp.text
    return resp.json()


def test_health(client):
    assert client.get("/health").json()["service"] == "report-automation-service"


def test_template_round_trips(client):
    created = _template(client)
    assert created["report_type"] == "financial_summary"
    assert client.get("/templates", headers=BOOK_ONE).json()[0]["id"] == created["id"]
    assert client.get("/templates", params={"report_type": "compliance"}, headers=BOOK_ONE).json() == []


def test_invalid_report_type_is_rejected(client):
    resp = client.post("/templates", params={"name": "x", "report_type": "adhoc"}, headers=BOOK_ONE)
    assert resp.status_code == 400


def test_run_generates_a_persisted_report(client):
    template = _template(client)
    schedule = _schedule(client, template["id"])
    report = client.post(f"/schedules/{schedule['id']}/run", params={"period": "2026-09"}, headers=BOOK_ONE)
    assert report.status_code == 200, report.text
    assert report.json()["status"] == "generated"
    stored = client.get("/reports", params={"schedule_id": schedule["id"]}, headers=BOOK_ONE).json()
    assert [r["period"] for r in stored] == ["2026-09"]
    after = client.get("/schedules", headers=BOOK_ONE).json()[0]
    assert after["last_run"] is not None


def test_pause_blocks_generation_and_resume_restores_it(client):
    template = _template(client)
    schedule = _schedule(client, template["id"])
    assert client.put(f"/schedules/{schedule['id']}/pause", headers=BOOK_ONE).json()["status"] == "paused"
    blocked = client.post(f"/schedules/{schedule['id']}/run", headers=BOOK_ONE)
    assert blocked.status_code == 400
    assert client.put(f"/schedules/{schedule['id']}/resume", headers=BOOK_ONE).json()["status"] == "active"
    assert client.post(f"/schedules/{schedule['id']}/run", headers=BOOK_ONE).status_code == 200


def test_one_book_never_sees_another_books_reports(client):
    template = _template(client, BOOK_ONE)
    schedule = _schedule(client, template["id"], BOOK_ONE)
    client.post(f"/schedules/{schedule['id']}/run", headers=BOOK_ONE)
    assert client.get("/templates", headers=BOOK_TWO).json() == []
    assert client.get("/schedules", headers=BOOK_TWO).json() == []
    assert client.get("/reports", headers=BOOK_TWO).json() == []
    other = client.post(
        "/schedules",
        params={"template_id": template["id"], "name": "x", "cron_expression": "* * * * *"},
        headers=BOOK_TWO,
    )
    assert other.status_code == 404
