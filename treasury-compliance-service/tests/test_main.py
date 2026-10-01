"""Book-scoping and persistence tests for treasury-compliance-service (fake Neo4j harness).

Covers: default check seeding + persistence, status updates with
remediation, report semantics, ownership and Book isolation.
"""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from treasury_compliance_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("tc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "tc-user-1", "tc-user-2"
BOOK_A, BOOK_B = "tc-book-a", "tc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def test_default_checks_seeded_and_persist():
    data = client.get("/checks/co-tc", headers=H1).json()
    assert data["total"] == 6
    assert data["compliant"] == 0
    assert data["compliance_rate"] == 0
    names = {c["check_name"] for c in data["checks"]}
    assert "Liquidity Coverage Ratio" in names
    assert "Segregation of Duties" in names
    assert all(c["status"] == "pending_review" for c in data["checks"])

    # seeded nodes persisted in the graph; not re-seeded on second access
    stored = [n for n in _fake_session.nodes if n.get("label") == "ComplianceCheck"]
    assert len(stored) == 6
    again = client.get("/checks/co-tc", headers=H1).json()
    assert again["total"] == 6
    assert {c["id"] for c in again["checks"]} == {c["id"] for c in data["checks"]}


def test_status_update_persists_with_remediation():
    checks = client.get("/checks/co-tc", headers=H1).json()["checks"]
    cid = checks[0]["id"]
    resp = client.put(
        f"/checks/{cid}/status", params={"status": "non_compliant", "remediation": "Fix required"}, headers=H1
    )
    assert resp.status_code == 200, resp.text
    assert resp.json() == {"check_id": cid, "status": "non_compliant"}

    after = {c["id"]: c for c in client.get("/checks/co-tc", headers=H1).json()["checks"]}
    assert after[cid]["status"] == "non_compliant"
    assert after[cid]["remediation"] == "Fix required"

    # report reflects the finding
    report = client.get("/report/co-tc", headers=H1).json()
    assert report["total_findings"] == 1
    assert report["findings"][0]["id"] == cid
    assert "Fix required" in report["recommendations"]
    assert report["compliance_rate"] == 0

    # mark compliant -> rate rises, finding cleared
    client.put(f"/checks/{cid}/status", params={"status": "compliant"}, headers=H1)
    report = client.get("/report/co-tc", headers=H1).json()
    assert report["compliance_rate"] == 1 / 6
    assert report["total_findings"] == 0


def test_update_scoping_404():
    checks = client.get("/checks/co-tc", headers=H1).json()["checks"]
    cid = checks[0]["id"]
    # cross-user and cross-Book updates 404
    assert client.put(f"/checks/{cid}/status", params={"status": "compliant"}, headers=H2).status_code == 404
    assert (
        client.put(
            f"/checks/{cid}/status", params={"status": "compliant"}, headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}
        ).status_code
        == 404
    )
    # status untouched
    after = {c["id"]: c for c in client.get("/checks/co-tc", headers=H1).json()["checks"]}
    assert after[cid]["status"] == "pending_review"


def test_book_a_b_isolation():
    client.get("/checks/co-tc", headers=H1)
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    client.get("/checks/co-tc", headers=hb)
    # each Book seeded its own 6 checks (distinct ids)
    a = {c["id"] for c in client.get("/checks/co-tc", headers=H1).json()["checks"]}
    b = {c["id"] for c in client.get("/checks/co-tc", headers=hb).json()["checks"]}
    assert len(a) == 6 and len(b) == 6
    assert a.isdisjoint(b)
    stored = [n for n in _fake_session.nodes if n.get("label") == "ComplianceCheck"]
    assert len(stored) == 12
    # other user sees nothing (no seeding for them on this company until they ask)
    assert client.get("/checks/co-tc", headers=H2).json()["total"] == 6
    other = {c["id"] for c in client.get("/checks/co-tc", headers=H2).json()["checks"]}
    assert other.isdisjoint(a)


def test_report_before_checks():
    r = client.get("/report/co-fresh", headers=H1).json()
    assert r["compliance_rate"] == 1.0
    assert r["findings"] == []
    assert r["recommendations"] == ["Run compliance checks first"]


def test_x_user_id_required():
    assert client.get("/checks/co-tc").status_code in (401, 403, 422)
    assert client.put("/checks/whatever/status", params={"status": "compliant"}).status_code in (401, 403, 422)
    assert client.get("/report/co-tc").status_code in (401, 403, 422)
