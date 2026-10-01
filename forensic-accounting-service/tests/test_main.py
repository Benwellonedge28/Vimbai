"""Book-scoping and persistence tests for forensic-accounting-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from forensic_accounting_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ta_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "au-fore-1", "au-fore-2"
BOOK_A, BOOK_B = "ab-fore-a", "ab-fore-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _engagement(company="co-ta", **kw):
    payload = {"company_id": company, "audit_type": "tax", "title": "Tax Compliance Audit 2026"}
    payload.update(kw)
    return payload


def test_create_and_list_engagements():
    resp = client.post("/engagements", json=_engagement(), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["status"] == "planned"
    assert data["book_id"] == BOOK_A

    listed = client.get("/engagements/co-ta", headers=H1).json()
    assert listed["total"] == 1
    assert listed["engagements"][0]["title"] == "Tax Compliance Audit 2026"

    # other user sees nothing
    assert client.get("/engagements/co-ta", headers=H2).json()["total"] == 0


def test_status_filter():
    client.post("/engagements", json=_engagement(), headers=H1)
    assert client.get("/engagements/co-ta", params={"status_filter": "planned"}, headers=H1).json()["total"] == 1
    assert client.get("/engagements/co-ta", params={"status_filter": "completed"}, headers=H1).json()["total"] == 0


def test_update_status_persists():
    eng = client.post("/engagements", json=_engagement(), headers=H1).json()
    resp = client.put(
        f"/engagements/{eng['id']}/status",
        params={"status": "completed", "summary": "All clear"},
        headers=H1,
    )
    assert resp.status_code == 200, resp.text
    assert resp.json()["status"] == "completed"
    stored = client.get("/engagements/co-ta", headers=H1).json()["engagements"][0]
    assert stored["status"] == "completed"
    assert stored["summary"] == "All clear"
    assert stored["end_date"]


def test_update_status_cross_user_404():
    eng = client.post("/engagements", json=_engagement(), headers=H1).json()
    blocked = client.put(f"/engagements/{eng['id']}/status", params={"status": "cancelled"}, headers=H2)
    assert blocked.status_code == 404
    blocked_book = client.put(
        f"/engagements/{eng['id']}/status",
        params={"status": "cancelled"},
        headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
    )
    assert blocked_book.status_code == 404
    stored = client.get("/engagements/co-ta", headers=H1).json()["engagements"][0]
    assert stored["status"] == "planned"


def test_findings_lifecycle():
    eng = client.post("/engagements", json=_engagement(), headers=H1).json()
    add = client.post(
        f"/engagements/{eng['id']}/findings",
        json={"title": "Under-reported income", "severity": "high", "description": "Income not fully reported"},
        headers=H1,
    )
    assert add.status_code == 200, add.text
    finding_id = add.json()["finding_id"]

    # cross-user add 404
    blocked = client.post(
        f"/engagements/{eng['id']}/findings",
        json={"title": "x", "description": "y"},
        headers=H2,
    )
    assert blocked.status_code == 404

    # remediate persists
    rem = client.put(
        f"/findings/{finding_id}/remediate",
        params={"remediation_note": "Amended return filed"},
        headers=H1,
    )
    assert rem.status_code == 200, rem.text
    assert rem.json()["status"] == "remediated"
    report = client.get(f"/report/{eng['id']}", headers=H1).json()
    assert report["remediated"] == 1
    assert report["open_findings"] == 0
    assert report["findings_summary"]["high"] == 1
    assert report["findings_summary"]["total"] == 1
    assert "Amended return filed" in report["engagement"]["findings"][0]["recommendation"]


def test_remediate_cross_user_404():
    eng = client.post("/engagements", json=_engagement(), headers=H1).json()
    add = client.post(
        f"/engagements/{eng['id']}/findings",
        json={"title": "f1", "description": "d"},
        headers=H1,
    ).json()
    blocked = client.put(f"/findings/{add['finding_id']}/remediate", headers=H2)
    assert blocked.status_code == 404


def test_report_cross_book_404():
    eng = client.post("/engagements", json=_engagement(), headers=H1).json()
    other = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get(f"/report/{eng['id']}", headers=other).status_code == 404
    # personal view still works
    assert client.get(f"/report/{eng['id']}", headers=H1_PERSONAL).status_code == 200


def test_book_a_b_isolation():
    client.post("/engagements", json=_engagement(company="co-a"), headers=H1)
    client.post("/engagements", json=_engagement(company="co-b"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    assert client.get("/engagements/co-a", headers=H1).json()["total"] == 1
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/engagements/co-a", headers=other_book).json()["total"] == 0
    assert client.get("/engagements/co-a", headers=H1_PERSONAL).json()["total"] == 1
    assert client.get("/engagements/co-b", headers=H1_PERSONAL).json()["total"] == 1


def test_x_user_id_required():
    assert client.post("/engagements", json=_engagement()).status_code in (401, 403, 422)
