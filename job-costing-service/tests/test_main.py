"""Book-scoping and persistence tests for job-costing-service (fake Neo4j harness)."""

import importlib.util
import os

import pytest
from fastapi.testclient import TestClient
from job_costing_service.database import Neo4jConnector
from main import app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("job_costing_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession

_fake_session = FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "jc-user-1", "jc-user-2"
BOOK_A, BOOK_B = "jc-book-a", "jc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_job(company="co-jc", name="Build deck", value=10000.0):
    return {"company_id": company, "job_name": name, "customer": "ACME", "contract_value": value}


def _create_job(payload=None, headers=H1):
    resp = client.post("/jobs", json=payload or _mk_job(), headers=headers)
    assert resp.status_code == 200, resp.text
    return resp.json()


def test_create_and_list_job():
    job = _create_job()
    assert job["contract_value"] == 10000.0
    listed = client.get("/jobs/co-jc", headers=H1).json()
    assert listed["total"] == 1
    assert listed["jobs"][0]["job_name"] == "Build deck"


def test_cost_rollup_and_profit():
    job = _create_job()
    jid = job["id"]
    r1 = client.post(f"/jobs/{jid}/costs", json={"cost_type": "materials", "amount": 2000.0}, headers=H1)
    assert r1.status_code == 200, r1.text
    assert r1.json()["total_cost"] == 2000.0
    r2 = client.post(f"/jobs/{jid}/costs", json={"cost_type": "labor", "amount": 3000.0}, headers=H1)
    body = r2.json()
    assert body["total_cost"] == 5000.0
    assert body["gross_profit"] == 5000.0
    assert body["margin"] == 50.0


def test_unknown_cost_type_stored_not_aggregated():
    job = _create_job()
    jid = job["id"]
    resp = client.post(f"/jobs/{jid}/costs", json={"cost_type": "misc", "amount": 999.0}, headers=H1)
    assert resp.status_code == 200
    assert resp.json()["total_cost"] == 0.0


def test_add_cost_to_missing_job_404():
    resp = client.post("/jobs/nope/costs", json={"cost_type": "materials", "amount": 1.0}, headers=H1)
    assert resp.status_code == 404


def test_status_filter():
    _create_job({"company_id": "co-jc", "job_name": "A", "status": "completed"})
    _create_job({"company_id": "co-jc", "job_name": "B"})
    all_jobs = client.get("/jobs/co-jc", headers=H1).json()
    assert all_jobs["total"] == 2
    done = client.get("/jobs/co-jc", params={"status": "completed"}, headers=H1).json()
    assert done["total"] == 1
    assert done["jobs"][0]["job_name"] == "A"


def test_user_isolation():
    job = _create_job()
    assert client.get("/jobs/co-jc", headers=H2).json()["total"] == 0
    r = client.post(f"/jobs/{job['id']}/costs", json={"cost_type": "labor", "amount": 5.0}, headers=H2)
    assert r.status_code == 404


def test_book_a_b_isolation():
    _create_job()
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/jobs/co-jc", headers=other).json()["total"] == 0


def test_personal_view_spans_books():
    _create_job(_mk_job(company="co-a"), headers=H1)
    _create_job(_mk_job(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/jobs/co-a", headers=H1_PERSONAL).json()["total"] == 1
    assert client.get("/jobs/co-b", headers=H1_PERSONAL).json()["total"] == 1


def test_book_cannot_see_other_books_records():
    """A Book request must not see records created in another Book."""
    _create_job(_mk_job(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/jobs/co-b", headers=H1).json()["total"] == 0


def test_profitability():
    j1 = _create_job({"company_id": "co-jc", "job_name": "A", "contract_value": 10000.0})
    _create_job({"company_id": "co-jc", "job_name": "B", "contract_value": 4000.0, "status": "completed"})
    client.post(f"/jobs/{j1['id']}/costs", json={"cost_type": "materials", "amount": 2000.0}, headers=H1)
    p = client.get("/jobs/co-jc/profitability", headers=H1).json()
    assert p["total_jobs"] == 2
    assert p["completed"] == 1
    assert p["total_contract_value"] == 14000.0
    assert p["total_cost"] == 2000.0
    # j1 profit = 10000 - 2000; j2 has no cost entries yet so its stored
    # aggregate stays 0 (original semantics: aggregates recomputed on add_cost)
    assert p["total_profit"] == 8000.0


def test_x_user_id_required():
    assert client.post("/jobs", json=_mk_job()).status_code in (401, 403, 422)
