"""Book-scoping, persistence and integrity tests for audit-compliance-service (fake Neo4j harness).

Covers: immutable event logging, per-caller integrity chain, checksum
verification round-trip, version snapshots, lineage derivation, event
search/analytics, ownership and Book isolation, cross-scope 404s, and
the pure planning/report/compliance endpoints.
"""

import importlib.util
import os

import main  # noqa: F401  (isort: keep bare main import first)
import pytest
from audit_compliance_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("audc_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "audc-user-1", "audc-user-2"
BOOK_A, BOOK_B = "audc-book-a", "audc-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}
HB = {"X-User-Id": U1, "X-Book-ID": BOOK_B}


def _event(resource_id="res-1", actor="actor-1", correlation_id=None, action="create"):
    return {
        "event_type": "create",
        "resource_type": "invoice",
        "resource_id": resource_id,
        "user_id": actor,
        "user_email": f"{actor}@example.com",
        "organization_id": "org-1",
        "action_details": {"action": action},
        "previous_state": None,
        "new_state": {"status": "posted"},
        "metadata": {"source": "test"},
        "ip_address": "10.0.0.1",
        "user_agent": "pytest",
        "session_id": "sess-1",
        "correlation_id": correlation_id,
    }


def _log(payload, headers=H1):
    r = client.post("/events", json=payload, headers=headers)
    assert r.status_code == 201, r.text
    return r.json()


def test_event_persistence_and_roundtrip_checksum():
    ev = _log(_event())
    event_id = ev["id"]
    assert ev["user_id"] == "actor-1"  # actor data preserved under the caller stamp

    # persisted and retrievable
    got = client.get(f"/events/{event_id}", headers=H1).json()
    assert got["id"] == event_id
    assert got["new_state"] == {"status": "posted"}

    # checksum verifies after the store round-trip
    v = client.get(f"/integrity/verify-event/{event_id}", headers=H1).json()
    assert v["matches"] is True


def test_cross_user_and_book_isolation():
    ev1 = _log(_event(resource_id="res-1"), headers=H1)
    ev2 = _log(_event(resource_id="res-2"), headers=H2)
    evb = _log(_event(resource_id="res-3"), headers=HB)

    # cross-user 404, other user's events invisible in listings
    assert client.get(f"/events/{ev1['id']}", headers=H2).status_code == 404
    assert client.get(f"/events/{ev2['id']}", headers=H1).status_code == 404
    listing = client.get("/events", headers=H1).json()
    assert listing["total"] == 1 and listing["events"][0]["resource_id"] == "res-1"

    # Book filter: caller with a different book does not see res-1
    assert client.get(f"/events/{ev1['id']}", headers=HB).status_code == 404
    assert client.get(f"/events/{evb['id']}", headers=H1).status_code == 404


def test_integrity_chain_per_caller():
    for i in range(3):
        _log(_event(resource_id=f"res-{i}"))
    _log(_event(resource_id="other-user"), headers=H2)

    v = client.get("/integrity/verify", headers=H1).json()
    assert v["total_events"] == 3  # caller-scoped, not global
    assert v["chain_length"] == 3
    assert v["is_valid"] is True
    assert v["errors"] == []
    assert len(v["chain_hash"]) == 64

    # chain hash is stable and order-dependent per caller
    v2 = client.get("/integrity/verify", headers=H1).json()
    assert v2["chain_hash"] == v["chain_hash"]


def test_event_filters_search_and_summary():
    _log(_event(resource_id="invoice-1", actor="alice", action="post"))
    _log(_event(resource_id="invoice-2", actor="bob", action="void"))

    r = client.get("/events", params={"resource_id": "invoice-1"}, headers=H1).json()
    assert r["total"] == 1 and r["events"][0]["user_id"] == "alice"

    r = client.get("/events", params={"user_id": "bob"}, headers=H1).json()
    assert r["total"] == 1 and r["events"][0]["resource_id"] == "invoice-2"

    # search across action_details (dict field)
    s = client.get("/search", params={"query": "void"}, headers=H1).json()
    assert s["total"] == 1 and s["events"][0]["resource_id"] == "invoice-2"

    summary = client.get("/analytics/summary", headers=H1).json()
    assert summary["total_events"] == 2
    assert summary["by_event_type"] == {"create": 2}
    assert summary["unique_users"] == 2


def test_version_snapshots_numbering_and_isolation():
    r1 = client.post(
        "/versions/invoice/res-1",
        params={"changed_by": "alice"},
        json={"status": "draft"},
        headers=H1,
    )
    assert r1.status_code == 200, r1.text
    assert r1.json()["version"] == 1

    r2 = client.post(
        "/versions/invoice/res-1",
        params={"changed_by": "alice", "change_reason": "post"},
        json={"status": "posted"},
        headers=H1,
    )
    assert r2.json()["version"] == 2

    history = client.get("/versions/invoice/res-1", headers=H1).json()
    assert [v["version"] for v in history["versions"]] == [1, 2]
    assert history["versions"][1]["state"] == {"status": "posted"}

    # other caller starts their own numbering and sees no foreign snapshots
    r3 = client.post(
        "/versions/invoice/res-1",
        params={"changed_by": "bob"},
        json={"status": "draft"},
        headers=H2,
    )
    assert r3.json()["version"] == 1
    assert client.get("/versions/invoice/res-1", headers=H2).json()["versions"][0]["version"] == 1


def test_lineage_derived_from_events():
    root = _log(_event(resource_id="doc-1", action="create"))
    child = _log(_event(resource_id="doc-1", correlation_id=root["id"], action="update"))

    lin = client.get("/lineage/doc-1", headers=H1).json()
    assert lin["total_events"] == 2
    assert len(lin["nodes"]) == 2
    assert len(lin["edges"]) == 1  # child -> root caused_by edge
    assert lin["edges"][0]["from_node_id"] == root["id"]

    # lineage is caller-scoped
    assert client.get("/lineage/doc-1", headers=H2).json().get("message") == "No lineage data found"


def test_pure_endpoints_unchanged():
    plan = {
        "audit_id": "aud-1",
        "company_id": "acme",
        "fiscal_year": "2026",
        "prior_year_findings": [{"risk_level": "high"}],
        "industry_risk_factors": ["credit"],
        "regulatory_requirements": ["IFRS"],
        "client_acceptance": True,
    }
    r = client.post("/planning/plan", json=plan, headers=H1)
    assert r.status_code == 200 and "materiality" in r.json()

    trails = {
        "company_id": "acme",
        "entries": [
            {
                "entry_id": "e1",
                "user_id": "alice",
                "action": "post",
                "timestamp": "2026-10-01T10:00:00Z",
                "system": "ledger",
            }
        ],
        "start_date": "2026-10-01",
        "end_date": "2026-10-02",
    }
    r = client.post("/trails/analyze", json=trails, headers=H1)
    assert r.status_code == 200

    sox = {
        "company_id": "acme",
        "fiscal_year": 2026,
        "controls": [{"control_id": "c1", "name": "Segregation of duties", "effective": True}],
        "evidence_required": ["access_review"],
    }
    r = client.post("/compliance/sox", json=sox, headers=H1)
    assert r.status_code == 200, r.text


def test_compliance_report_over_caller_events():
    _log(_event(resource_id="rep-1", actor="alice"))
    _log(_event(resource_id="rep-2", actor="bob"))

    req = {
        "start_date": "2026-01-01T00:00:00Z",
        "end_date": "2030-01-01T00:00:00Z",
        "resource_types": [],
        "user_ids": [],
        "include_verifications": True,
        "include_integrity_checks": True,
    }
    r = client.post("/compliance/report", json=req, headers=H1)
    assert r.status_code == 200, r.text
    body = r.json()
    assert body["total_events"] == 2
    assert body["integrity_verified"] is True
    assert body["findings"] == []
    assert body["events_by_user"] == {"alice": 1, "bob": 1}

    # other caller's report is empty (caller-scoped)
    r2 = client.post("/compliance/report", json=req, headers=H2)
    assert r2.json()["total_events"] == 0
