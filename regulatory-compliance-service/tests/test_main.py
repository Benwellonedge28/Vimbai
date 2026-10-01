"""Book-scoping and persistence tests for regulatory-compliance-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from regulatory_compliance_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("rc_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "rc-user-1", "rc-user-2"
BOOK_A, BOOK_B = "rc-book-a", "rc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _payload(company="co-rc", **kw):
    p = {
        "company_id": company,
        "regulation_name": "IFRS 15 Revenue",
        "jurisdiction": "ZW",
        "framework": "IFRS",
        "requirement": "Recognize revenue when performance obligation satisfied",
        "risk_if_non_compliant": "high",
    }
    p.update(kw)
    return p


def test_create_list_persist():
    resp = client.post("/regulations", json=_payload(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["status"] == "pending_review"

    listed = client.get("/regulations", params={"company_id": "co-rc"}, headers=H1).json()
    assert len(listed) == 1
    assert listed[0]["id"] == data["id"]
    # framework filter
    assert len(client.get("/regulations", params={"company_id": "co-rc", "framework": "IFRS"}, headers=H1).json()) == 1
    assert client.get("/regulations", params={"company_id": "co-rc", "framework": "SOX"}, headers=H1).json() == []
    # other user sees nothing
    assert client.get("/regulations", params={"company_id": "co-rc"}, headers=H2).json() == []


def test_update_status_persists():
    rid = client.post("/regulations", json=_payload(), headers=H1).json()["id"]
    upd = client.post(f"/regulations/{rid}/update", params={"company_id": "co-rc", "status": "compliant"}, headers=H1)
    assert upd.status_code == 200, upd.text
    assert upd.json() == {"updated": True, "regulation_id": rid, "status": "compliant"}
    stored = client.get("/regulations", params={"company_id": "co-rc"}, headers=H1).json()
    assert stored[0]["status"] == "compliant"
    assert stored[0]["last_reviewed"]

    # invalid status keeps current status but still stamps review date
    upd2 = client.post(f"/regulations/{rid}/update", params={"company_id": "co-rc", "status": "bogus"}, headers=H1)
    assert upd2.json()["status"] == "compliant"


def test_update_scopes():
    rid = client.post("/regulations", json=_payload(), headers=H1).json()["id"]
    # cross-user
    assert (
        client.post(
            f"/regulations/{rid}/update", params={"company_id": "co-rc", "status": "compliant"}, headers=H2
        ).status_code
        == 404
    )
    # cross-Book
    assert (
        client.post(
            f"/regulations/{rid}/update",
            params={"company_id": "co-rc", "status": "compliant"},
            headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
        ).status_code
        == 404
    )
    # wrong company
    assert (
        client.post(
            f"/regulations/{rid}/update", params={"company_id": "other-co", "status": "compliant"}, headers=H1
        ).status_code
        == 404
    )


def test_dashboard_semantics():
    client.post("/regulations", json=_payload(framework="IFRS"), headers=H1)
    rid = client.post(
        "/regulations",
        json=_payload(regulation_name="SOX 404", framework="SOX", risk_if_non_compliant="medium"),
        headers=H1,
    ).json()["id"]
    client.post(f"/regulations/{rid}/update", params={"company_id": "co-rc", "status": "compliant"}, headers=H1)
    d = client.get("/dashboard", params={"company_id": "co-rc"}, headers=H1).json()
    assert d["total_regulations"] == 2
    assert d["compliant"] == 1
    assert d["pending"] == 1
    assert d["compliance_rate"] == 50.0
    assert d["by_framework"]["IFRS"] == {"total": 1, "compliant": 0, "non_compliant": 0}
    assert d["by_jurisdiction"]["ZW"] == {"total": 2, "compliant": 1}
    # high-risk non-compliant item surfaces in critical items
    assert len(d["critical_items"]) == 1
    assert d["critical_items"][0]["regulation"] == "IFRS 15 Revenue"
    # empty company rates 100
    assert client.get("/dashboard", params={"company_id": "no-such"}, headers=H1).json()["compliance_rate"] == 100


def test_book_a_b_isolation():
    client.post("/regulations", json=_payload(regulation_name="book-a"), headers=H1)
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    client.post("/regulations", json=_payload(regulation_name="book-b"), headers=hb)
    assert len(client.get("/regulations", params={"company_id": "co-rc"}, headers=H1).json()) == 1
    assert len(client.get("/regulations", params={"company_id": "co-rc"}, headers=hb).json()) == 1
    assert len(client.get("/regulations", params={"company_id": "co-rc"}, headers=H1_PERSONAL).json()) == 2
    # dashboards are per-Book
    assert client.get("/dashboard", params={"company_id": "co-rc"}, headers=H1).json()["total_regulations"] == 1
    assert client.get("/dashboard", params={"company_id": "co-rc"}, headers=hb).json()["total_regulations"] == 1


def test_x_user_id_required():
    assert client.post("/regulations", json=_payload()).status_code in (401, 403, 422)
    assert client.get("/regulations", params={"company_id": "co-rc"}).status_code in (401, 403, 422)
    assert client.get("/dashboard", params={"company_id": "co-rc"}).status_code in (401, 403, 422)
