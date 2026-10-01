"""Book-scoping and persistence tests for fraud-detection-service (fake Neo4j harness).

Covers: detection engine over persisted rules, alert persistence and
status lifecycle, rule seeding/add/toggle, risk assessment, ownership
and Book isolation.
"""

import importlib.util
import os
from datetime import datetime, timezone

import main
import pytest
from fastapi.testclient import TestClient
from fraud_detection_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("fd_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "fd-user-1", "fd-user-2"
BOOK_A, BOOK_B = "fd-book-a", "fd-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}

NOON = datetime(2026, 1, 15, 12, 0, 0, tzinfo=timezone.utc)


def _tx(amount=1000, company="co-fd", merchant="Staples", description="Office supplies", ts=None):
    return {
        "company_id": company,
        "account_id": "acc-1",
        "amount": amount,
        "currency": "USD",
        "timestamp": (ts or NOON).isoformat(),
        "description": description,
        "merchant": merchant,
    }


def test_detect_and_alert_persistence():
    resp = client.post("/detect", json={"company_id": "co-fd", "transactions": [_tx(amount=80000)]}, headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["transactions_analyzed"] == 1
    assert data["fraudulent_detected"] == 1
    assert data["risk_assessment"]["overall_risk_level"] in ("high", "extreme")
    # 80000 trips both the large-amount and round-amount rules
    assert len(data["alerts"]) == 2
    alert = next(a for a in data["alerts"] if a["rule_name"] == "Large Transaction Alert")
    assert alert["severity"] == "high"
    assert alert["status"] == "pending"

    # alerts persist and are retrievable
    stored = client.get("/alerts/co-fd", headers=H1).json()
    assert stored["total"] == 2
    assert any(a["id"] == alert["id"] for a in stored["alerts"])
    # other user sees nothing
    assert client.get("/alerts/co-fd", headers=H2).json()["total"] == 0


def test_detect_no_fraud():
    resp = client.post(
        "/detect",
        json={
            "company_id": "co-fd",
            "transactions": [_tx(amount=250), _tx(amount=500, merchant="Restaurant", description="Lunch")],
        },
        headers=H1,
    )
    assert resp.status_code == 200
    data = resp.json()
    assert data["fraudulent_detected"] == 0
    assert data["alerts"] == []
    assert data["risk_assessment"]["overall_risk_level"] == "minimal"


def test_detect_requires_transactions():
    assert client.post("/detect", json={"company_id": "co-fd", "transactions": []}, headers=H1).status_code == 400


def test_alert_status_lifecycle_and_scoping():
    alert = next(
        a
        for a in client.post(
            "/detect", json={"company_id": "co-fd", "transactions": [_tx(amount=80000)]}, headers=H1
        ).json()["alerts"]
        if a["rule_name"] == "Large Transaction Alert"
    )
    upd = client.put(f"/alerts/{alert['id']}/status", params={"new_status": "confirmed"}, headers=H1)
    assert upd.status_code == 200, upd.text
    assert upd.json() == {"alert_id": alert["id"], "status": "confirmed"}
    stored = client.get("/alerts/co-fd", headers=H1).json()
    assert any(a["status"] == "confirmed" for a in stored["alerts"])
    # status filter (the round-amount co-alert stays pending)
    assert client.get("/alerts/co-fd", params={"status_filter": "pending"}, headers=H1).json()["total"] == 1
    assert client.get("/alerts/co-fd", params={"status_filter": "confirmed"}, headers=H1).json()["total"] == 1

    # cross-user and cross-Book updates 404
    assert (
        client.put(f"/alerts/{alert['id']}/status", params={"new_status": "false_positive"}, headers=H2).status_code
        == 404
    )
    assert (
        client.put(
            f"/alerts/{alert['id']}/status",
            params={"new_status": "false_positive"},
            headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
        ).status_code
        == 404
    )
    assert client.put("/alerts/no-such-alert/status", params={"new_status": "confirmed"}, headers=H1).status_code == 404


def test_rules_seed_add_toggle():
    rules = client.get("/rules/co-fd", headers=H1).json()["rules"]
    assert len(rules) == 6  # DEFAULT_RULES seeded
    names = {r["name"] for r in rules}
    assert "Large Transaction Alert" in names and "Off-Hours Transaction" in names

    # seeding persists - second call returns same rules
    again = client.get("/rules/co-fd", headers=H1).json()["rules"]
    assert {r["id"] for r in again} == {r["id"] for r in rules}

    # add a custom rule
    added = client.post(
        "/rules/co-fd",
        json={
            "name": "ZWL cash ban",
            "description": "No cash transactions allowed",
            "rule_type": "amount_threshold",
            "parameters": {"threshold": 1.0},
        },
        headers=H1,
    )
    assert added.status_code == 200, added.text
    assert added.json()["status"] == "added"
    rid = added.json()["rule_id"]
    all_rules = client.get("/rules/co-fd", headers=H1).json()["rules"]
    assert len(all_rules) == 7

    # toggle it off (Book-gated)
    tog = client.put(f"/rules/{rid}", params={"enabled": False}, headers=H1)
    assert tog.status_code == 200
    assert tog.json() == {"rule_id": rid, "enabled": False}
    assert [r for r in client.get("/rules/co-fd", headers=H1).json()["rules"] if r["id"] == rid][0]["enabled"] is False

    # cross-user toggle 404; the other user gets their own seeded set, never ours
    assert client.put(f"/rules/{rid}", params={"enabled": True}, headers=H2).status_code == 404
    other_rules = client.get("/rules/co-fd", headers=H2).json()["rules"]
    assert len(other_rules) == 6
    assert rid not in {r["id"] for r in other_rules}


def test_risk_assessment():
    # no alerts -> minimal
    empty = client.get("/risk/co-fd", headers=H1).json()
    assert empty["overall_risk_level"] == "minimal"
    assert empty["risk_score"] == 0

    client.post("/detect", json={"company_id": "co-fd", "transactions": [_tx(amount=80000)]}, headers=H1)
    risk = client.get("/risk/co-fd", headers=H1).json()
    assert risk["total_transactions"] == 1
    assert risk["flagged_transactions"] == 1
    assert risk["risk_score"] > 0
    assert risk["overall_risk_level"] in ("high", "extreme")
    # other user still minimal
    assert client.get("/risk/co-fd", headers=H2).json()["overall_risk_level"] == "minimal"


def test_book_a_b_isolation():
    client.post("/detect", json={"company_id": "co-fd", "transactions": [_tx(amount=55555)]}, headers=H1)
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    client.post("/detect", json={"company_id": "co-fd", "transactions": [_tx(amount=95555)]}, headers=hb)
    assert client.get("/alerts/co-fd", headers=H1).json()["total"] == 1
    assert client.get("/alerts/co-fd", headers=hb).json()["total"] == 1
    # personal view spans both Books
    assert client.get("/alerts/co-fd", headers=H1_PERSONAL).json()["total"] == 2
    # rules are seeded per Book scope independently
    assert len(client.get("/rules/co-fd", headers=H1).json()["rules"]) == 6
    assert len(client.get("/rules/co-fd", headers=hb).json()["rules"]) == 6


def test_x_user_id_required():
    assert client.post("/detect", json={"company_id": "co-fd", "transactions": [_tx()]}).status_code in (401, 403, 422)
    assert client.get("/alerts/co-fd").status_code in (401, 403, 422)
    assert client.get("/rules/co-fd").status_code in (401, 403, 422)
    assert client.get("/risk/co-fd").status_code in (401, 403, 422)
