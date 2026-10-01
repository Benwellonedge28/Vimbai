"""Book-scoping and persistence tests for bank-relationship-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from bank_relationship_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("br_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "br-user-1", "br-user-2"
BOOK_A, BOOK_B = "br-book-a", "br-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _rel(company="co-br", bank="CBZ Bank", **kw):
    payload = {"company_id": company, "bank_name": bank, "services": ["checking", "credit_line"]}
    payload.update(kw)
    return payload


def test_create_and_list_relationships():
    resp = client.post("/relationships", json=_rel(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["status"] == "active"
    assert data["book_id"] == BOOK_A
    assert data["rating"] == 3

    listed = client.get("/relationships/co-br", headers=H1).json()
    assert listed["total"] == 1
    assert listed["relationships"][0]["bank_name"] == "CBZ Bank"

    # other user sees nothing
    assert client.get("/relationships/co-br", headers=H2).json()["total"] == 0

    # status filter
    assert client.get("/relationships/co-br", params={"status_filter": "active"}, headers=H1).json()["total"] == 1
    assert client.get("/relationships/co-br", params={"status_filter": "terminated"}, headers=H1).json()["total"] == 0


def test_update_relationship_persists_and_scopes():
    rel_id = client.post("/relationships", json=_rel(), headers=H1).json()["id"]
    upd = client.put(
        f"/relationships/{rel_id}", params={"rating": 5, "status": "dormant", "notes": "Reduced activity"}, headers=H1
    )
    assert upd.status_code == 200, upd.text
    assert upd.json() == {"id": rel_id, "rating": 5, "status": "dormant"}

    stored = client.get("/relationships/co-br", headers=H1).json()["relationships"][0]
    assert stored["rating"] == 5
    assert stored["status"] == "dormant"
    assert stored["notes"] == "Reduced activity"

    # cross-user and cross-Book update 404
    assert client.put(f"/relationships/{rel_id}", params={"rating": 1}, headers=H2).status_code == 404
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.put(f"/relationships/{rel_id}", params={"rating": 1}, headers=other_book).status_code == 404
    # original untouched
    assert client.get("/relationships/co-br", headers=H1).json()["relationships"][0]["rating"] == 5


def test_quality_metrics_scoped_to_relationship():
    rel_id = client.post("/relationships", json=_rel(), headers=H1).json()["id"]
    add = client.post(
        "/quality-metrics",
        json={"relationship_id": rel_id, "metric_name": "response_time", "score": 4},
        headers=H1,
    )
    assert add.status_code == 200, add.text
    client.post(
        "/quality-metrics",
        json={"relationship_id": rel_id, "metric_name": "fee_competitiveness", "score": 2},
        headers=H1,
    )

    data = client.get(f"/quality-metrics/{rel_id}", headers=H1).json()
    assert data["avg_score"] == 3.0
    assert len(data["metrics"]) == 2

    # metrics for an invisible relationship 404 (other user / other Book)
    assert client.get(f"/quality-metrics/{rel_id}", headers=H2).status_code == 404
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get(f"/quality-metrics/{rel_id}", headers=other_book).status_code == 404
    blocked = client.post(
        "/quality-metrics",
        json={"relationship_id": rel_id, "metric_name": "x", "score": 1},
        headers=H2,
    )
    assert blocked.status_code == 404


def test_summary_scoped():
    client.post("/relationships", json=_rel(company="co-s", bank="CBZ Bank", rating=4), headers=H1)
    client.post(
        "/relationships",
        json=_rel(company="co-s", bank="Stanbic Bank", services=["fx"]),
        headers=H1,
    )
    summary = client.get("/summary/co-s", headers=H1).json()
    assert summary["total_relationships"] == 2
    assert summary["active"] == 2
    assert summary["unique_banks"] == 2
    assert summary["total_services"] == 3
    assert summary["avg_rating"] == 3.5

    # other user: empty summary
    other = client.get("/summary/co-s", headers=H2).json()
    assert other["total_relationships"] == 0


def test_book_a_b_isolation():
    client.post("/relationships", json=_rel(company="co-a"), headers=H1)
    client.post("/relationships", json=_rel(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/relationships/co-a", headers=H1).json()["total"] == 1
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/relationships/co-a", headers=other_book).json()["total"] == 0
    # personal spans books
    assert client.get("/relationships/co-a", headers=H1_PERSONAL).json()["total"] == 1
    assert client.get("/relationships/co-b", headers=H1_PERSONAL).json()["total"] == 1


def test_x_user_id_required():
    assert client.post("/relationships", json=_rel()).status_code in (401, 403, 422)
    assert client.get("/relationships/co-br").status_code in (401, 403, 422)
