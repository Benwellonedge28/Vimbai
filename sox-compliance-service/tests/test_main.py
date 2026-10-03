"""Book-scoping and persistence tests for sox-compliance-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from sox_compliance_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("sox_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "sox-user-1", "sox-user-2"
BOOK_A, BOOK_B = "sox-book-a", "sox-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _mk_control(headers):
    return client.post(
        "/controls",
        params={
            "control_id_ref": "SOX-ITGC-001",
            "description": "Quarterly access review",
            "control_type": "detective",
            "control_nature": "automated",
            "frequency": "quarterly",
            "owner": "IT Audit",
            "process": "IT General Controls",
        },
        headers=headers,
    )


def test_health():
    assert client.get("/health").json()["status"] == "healthy"


def test_control_isolation_and_filters():
    r = _mk_control(H1)
    assert r.status_code == 200, r.text
    cid = r.json()["id"]
    assert r.json()["control_id_ref"] == "SOX-ITGC-001"

    _mk_control(H2)

    assert len(client.get("/controls", headers=H1).json()) == 1
    assert len(client.get("/controls", headers=H2).json()) == 1
    assert client.get("/controls", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json() == []

    assert len(client.get("/controls", params={"process": "IT General Controls"}, headers=H1).json()) == 1
    assert len(client.get("/controls", params={"process": "Payroll"}, headers=H1).json()) == 0
    assert len(client.get("/controls", params={"status": "active"}, headers=H1).json()) == 1
    assert len(client.get("/controls", params={"status": "retired"}, headers=H1).json()) == 0


def test_control_test_and_auto_deficiency():
    cid = _mk_control(H1).json()["id"]

    # cross-user test: control invisible -> 404
    r = client.post(
        f"/controls/{cid}/test",
        params={"test_period": "Q1-2026", "tester": "Ext Auditor", "sample_size": 40, "exceptions_found": 0},
        headers=H2,
    )
    assert r.status_code == 404

    # pass
    r = client.post(
        f"/controls/{cid}/test",
        params={"test_period": "Q1-2026", "tester": "Auditor A", "sample_size": 40, "exceptions_found": 0},
        headers=H1,
    )
    assert r.status_code == 200, r.text
    assert r.json()["result"] == "pass"

    # pass_with_exception boundary: 1 exception of 40 = 2.5% < 10%
    r = client.post(
        f"/controls/{cid}/test",
        params={"test_period": "Q2-2026", "tester": "Auditor A", "sample_size": 40, "exceptions_found": 1},
        headers=H1,
    )
    assert r.json()["result"] == "pass_with_exception"

    # fail: 10/40 = 25% >= 10% -> significant_deficiency (>20%)
    r = client.post(
        f"/controls/{cid}/test",
        params={"test_period": "Q3-2026", "tester": "Auditor A", "sample_size": 40, "exceptions_found": 10},
        headers=H1,
    )
    assert r.json()["result"] == "fail"

    tests = client.get(f"/controls/{cid}/tests", headers=H1).json()
    assert len(tests) == 3
    assert client.get(f"/controls/{cid}/tests", headers=H2).json() == []

    # auto deficiency created, scoped
    defs = client.get("/deficiencies", headers=H1).json()
    assert len(defs) == 1
    assert defs[0]["severity"] == "significant_deficiency"
    assert client.get("/deficiencies", headers=H2).json() == []

    # fail with exceptions between 10-20%: control_deficiency
    r = client.post(
        f"/controls/{cid}/test",
        params={"test_period": "Q4-2026", "tester": "Auditor A", "sample_size": 40, "exceptions_found": 5},
        headers=H1,
    )
    assert r.json()["result"] == "fail"
    defs = client.get("/deficiencies", headers=H1).json()
    assert len(defs) == 2
    assert defs[1]["severity"] == "control_deficiency"


def test_deficiency_lifecycle_and_dashboard():
    # manual deficiency (invalid severity contract kept)
    r = client.post(
        "/deficiencies",
        params={"control_id": "none", "severity": "super_bad", "description": "x"},
        headers=H1,
    )
    assert r.status_code == 400

    r = client.post(
        "/deficiencies",
        params={
            "control_id": "ctrl-x",
            "severity": "material_weakness",
            "description": "Segregation of duties gap",
            "remediation_plan": "Hire second operator",
        },
        headers=H1,
    )
    assert r.status_code == 200, r.text
    did = r.json()["id"]

    # cross-user update: 404
    r = client.put(f"/deficiencies/{did}", params={"status": "remediated"}, headers=H2)
    assert r.status_code == 404

    # owner update persists
    r = client.put(
        f"/deficiencies/{did}",
        params={"status": "remediated", "remediation_plan": "Resolved via hiring"},
        headers=H1,
    )
    assert r.status_code == 200, r.text
    assert r.json()["status"] == "remediated"
    assert r.json()["remediated_date"] is not None
    assert r.json()["remediation_plan"] == "Resolved via hiring"

    defs = client.get("/deficiencies", params={"status": "remediated"}, headers=H1).json()
    assert len(defs) == 1

    # dashboard scoped to caller
    _mk_control(H2)  # foreign control + auto deficiency noise
    client.post(
        "/deficiencies",
        params={"control_id": "ctrl-y", "severity": "material_weakness", "description": "foreign"},
        headers=H2,
    )
    d = client.get("/dashboard", headers=H1).json()
    assert d["total_controls"] == 0
    assert d["open_deficiencies"] == 0
    assert d["material_weaknesses"] == 0  # U1's only MW is remediated

    d2 = client.get("/dashboard", headers=H2).json()
    assert d2["total_controls"] == 1
    assert d2["material_weaknesses"] == 1
