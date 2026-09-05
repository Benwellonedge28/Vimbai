"""Book-scoping and persistence tests for risk-assessment-service (fake Neo4j harness)."""

import importlib.util
import os
import sys

import main
import pytest
from fastapi.testclient import TestClient
from main import app
from risk_assessment_service.database import Neo4jConnector

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("risk_assessment_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession

_fake_session = FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    """Each test starts from an empty graph (fresh per-request state)."""
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "risk-user-1", "risk-user-2"
BOOK_A, BOOK_B = "risk-book-a", "risk-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_risk(company="co-risk", name="Fraud exposure", likelihood=3, impact=4):
    return {
        "company_id": company,
        "category": "financial",
        "name": name,
        "description": "test risk",
        "likelihood": likelihood,
        "impact": impact,
        "owner": "cfo",
    }


def test_create_risk_computes_score_and_level():
    resp = client.post("/risks", json=_mk_risk(), headers=H1)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["risk_score"] == 12
    assert body["level"] == "high"
    return body["id"]


def test_book_isolation():
    r1 = client.post("/risks", json=_mk_risk(name="A risk"), headers=H1).json()
    client.post("/risks", json=_mk_risk(name="B risk"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})

    in_a = client.get("/risks/co-risk", headers=H1).json()
    assert [r["name"] for r in in_a["risks"]] == ["A risk"]
    in_b = client.get("/risks/co-risk", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()
    assert [r["name"] for r in in_b["risks"]] == ["B risk"]

    personal = client.get("/risks/co-risk", headers=H1_PERSONAL).json()
    assert len(personal["risks"]) == 2


def test_user_isolation():
    client.post("/risks", json=_mk_risk(name="U1 risk"), headers=H1)
    client.post("/risks", json=_mk_risk(name="U2 risk"), headers=H2)

    u1_view = client.get("/risks/co-risk", headers=H1).json()
    u2_view = client.get("/risks/co-risk", headers=H2).json()
    assert [r["name"] for r in u1_view["risks"]] == ["U1 risk"]
    assert [r["name"] for r in u2_view["risks"]] == ["U2 risk"]


def test_update_persists_and_recalculates():
    rid = client.post("/risks", json=_mk_risk(likelihood=2, impact=2), headers=H1).json()["id"]
    resp = client.put(f"/risks/{rid}", params={"likelihood": 5, "impact": 5, "status": "assessing"}, headers=H1)
    assert resp.status_code == 200, resp.text
    assert resp.json()["risk_score"] == 25
    assert resp.json()["level"] == "critical"

    after = client.get("/risks/co-risk", headers=H1).json()["risks"]
    stored = [r for r in after if r["id"] == rid][0]
    assert stored["likelihood"] == 5
    assert stored["status"] == "assessing"
    assert stored["level"] == "critical"


def test_cross_book_update_404():
    rid = client.post("/risks", json=_mk_risk(), headers=H1).json()["id"]
    resp = client.put(f"/risks/{rid}", params={"status": "monitoring"}, headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert resp.status_code == 404


def test_cross_user_update_404():
    rid = client.post("/risks", json=_mk_risk(), headers=H1).json()["id"]
    resp = client.put(f"/risks/{rid}", params={"status": "monitoring"}, headers=H2)
    assert resp.status_code == 404


def test_close_risk_persists():
    rid = client.post("/risks", json=_mk_risk(), headers=H1).json()["id"]
    resp = client.delete(f"/risks/{rid}", headers=H1)
    assert resp.status_code == 200, resp.text
    assert resp.json()["status"] == "closed"

    after = client.get("/risks/co-risk", headers=H1).json()["risks"]
    stored = [r for r in after if r["id"] == rid][0]
    assert stored["status"] == "closed"


def test_dashboard_scoped_to_book():
    client.post("/risks", json=_mk_risk(name="dash-a", likelihood=4, impact=5), headers=H1)
    client.post(
        "/risks", json=_mk_risk(name="dash-b", likelihood=1, impact=1), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}
    )

    dash = client.get("/dashboard/co-risk", headers=H1).json()
    assert dash["total_risks"] == 1
    assert dash["by_level"]["critical"] == 1
    assert dash["avg_score"] == 20
    assert [t["name"] for t in dash["top_risks"]] == ["dash-a"]


def test_category_and_level_filters():
    client.post("/risks", json=_mk_risk(name="f-fin", likelihood=4, impact=5), headers=H1)
    client.post("/risks", json=_mk_risk(company="co-risk", name="f-cyber", likelihood=1, impact=1), headers=H1)

    filtered = client.get("/risks/co-risk", params={"category": "financial"}, headers=H1).json()
    assert all(r["category"] == "financial" for r in filtered["risks"])

    low = client.get("/risks/co-risk", params={"level": "low"}, headers=H1).json()
    assert all(r["level"] == "low" for r in low["risks"])


def test_unknown_risk_update_404():
    resp = client.put("/risks/does-not-exist", params={"status": "monitoring"}, headers=H1)
    assert resp.status_code == 404


def test_personal_view_spans_books():
    client.post("/risks", json=_mk_risk(name="pv-a"), headers=H1)
    client.post("/risks", json=_mk_risk(name="pv-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    personal = client.get("/risks/co-risk", headers=H1_PERSONAL).json()
    names = sorted(r["name"] for r in personal["risks"] if r["name"] in ("pv-a", "pv-b"))
    assert names == ["pv-a", "pv-b"]
