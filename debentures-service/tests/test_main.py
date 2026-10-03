"""Book-scoping and persistence tests for debentures-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from debentures_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("deb_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "deb-user-1", "deb-user-2"
BOOK_A, BOOK_B = "deb-book-a", "deb-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}

MATURITY = "2030-06-30"


def _mk_class(client, headers, **kw):
    params = {
        "name": "Class A",
        "company_id": "co-1",
        "nominal_value": 100.0,
        "issue_price": 95.0,
        "coupon_rate": 10.0,
        "interest_payment_frequency": "semi_annual",
        "maturity_date": MATURITY,
        "redemption_price": 102.0,
    }
    params.update(kw)
    return client.post("/classes/create", params=params, headers=headers)


def test_health_and_root():
    assert client.get("/health").json()["status"] == "healthy"
    assert client.get("/").json()["service"] == "debentures-service"


def test_create_class_and_lists():
    r = _mk_class(client, H1)
    assert r.status_code == 200, r.text
    c = r.json()
    assert c["debentures_outstanding"] == 0

    # isolation: other user / other Book see nothing
    _mk_class(client, H2, name="Foreign")
    _mk_class(client, {"X-User-Id": U1, "X-Book-ID": BOOK_B}, name="Other Book")
    assert len(client.get("/classes", headers=H1).json()["debenture_classes"]) == 1
    assert len(client.get("/classes", headers=H2).json()["debenture_classes"]) == 1
    assert len(client.get("/classes", headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}).json()["debenture_classes"]) == 1
    # company filter still applies within the caller's visible set
    assert client.get("/classes", params={"company_id": "co-other"}, headers=H1).json()["debenture_classes"] == []
    assert len(client.get("/classes", params={"company_id": "co-1"}, headers=H1).json()["debenture_classes"]) == 1


def test_issue_and_class_counters():
    cls = _mk_class(client, H1).json()
    r = client.post(
        f"/classes/{cls['id']}/issue",
        params={"company_id": "co-1", "debentures_issued": 1000, "issue_date": "2026-01-15"},
        headers=H1,
    )
    assert r.status_code == 200, r.text
    issue = r.json()
    assert issue["total_proceeds"] == 1000 * 95.0
    assert issue["discount_on_issue"] == 1000 * 5.0

    # counters persisted on the class node
    got = client.get("/classes", headers=H1).json()["debenture_classes"][0]
    assert got["debentures_issued"] == 1000
    assert got["debentures_outstanding"] == 1000

    # issue against another user's class: original error shape, 200
    r2 = client.post(
        f"/classes/{cls['id']}/issue",
        params={"company_id": "co-1", "debentures_issued": 10, "issue_date": "2026-01-16"},
        headers=H2,
    )
    assert r2.json() == {"error": "Debenture class not found"}

    # caller's issues list: only their own
    assert len(client.get("/issues", headers=H1).json()["issues"]) == 1
    assert client.get("/issues", headers=H2).json()["issues"] == []


def test_interest_accrue_and_pay():
    cls = _mk_class(client, H1, coupon_rate=12.0, nominal_value=100.0, interest_payment_frequency="quarterly").json()
    r = client.post(
        f"/classes/{cls['id']}/issue",
        params={"company_id": "co-1", "debentures_issued": 1000, "issue_date": "2026-01-15"},
        headers=H1,
    )
    assert r.status_code == 200

    acc = client.post(
        f"/classes/{cls['id']}/interest/accrue",
        params={
            "company_id": "co-1",
            "period_start": "2026-03-01",
            "period_end": "2026-03-31",
            "debentures_outstanding": 1000,
        },
        headers=H1,
    )
    assert acc.status_code == 200, acc.text
    interest = acc.json()
    # annual 1000*100*0.12 = 12000; quarterly -> 3000; tax 20% -> 600; net 2400
    assert interest["interest_amount"] == 3000.0
    assert interest["tax_deducted"] == 600.0
    assert interest["net_payment"] == 2400.0
    assert interest["status"] == "accrued"

    # pay: status flips, payment_date persisted
    paid = client.post(f"/interest/{interest['id']}/pay", params={"payment_date": "2026-04-05"}, headers=H1).json()
    assert paid["status"] == "paid"

    listed = client.get("/interest", headers=H1).json()["interest_payments"]
    assert listed[0]["status"] == "paid"
    assert listed[0]["payment_date"] is not None

    # cross-user pay: original error shape
    r2 = client.post(f"/interest/{interest['id']}/pay", params={"payment_date": "2026-04-06"}, headers=H2)
    assert r2.json() == {"error": "Interest not found"}

    # isolation of /interest listing
    assert client.get("/interest", headers=H2).json()["interest_payments"] == []


def test_redeem_updates_outstanding():
    cls = _mk_class(client, H1).json()
    client.post(
        f"/classes/{cls['id']}/issue",
        params={"company_id": "co-1", "debentures_issued": 1000, "issue_date": "2026-01-15"},
        headers=H1,
    )
    r = client.post(
        f"/classes/{cls['id']}/redeem",
        params={"company_id": "co-1", "debentures_redeemed": 400, "redemption_date": "2026-09-30"},
        headers=H1,
    )
    assert r.status_code == 200, r.text
    redemption = r.json()
    assert redemption["total_proceeds"] == 400 * 102.0
    assert redemption["premium_on_redemption"] == 400 * 2.0

    got = client.get("/classes", headers=H1).json()["debenture_classes"][0]
    assert got["debentures_outstanding"] == 600

    # cross-user redeem: original error shape, no counter mutation
    r2 = client.post(
        f"/classes/{cls['id']}/redeem",
        params={"company_id": "co-1", "debentures_redeemed": 100, "redemption_date": "2026-10-01"},
        headers=H2,
    )
    assert r2.json() == {"error": "Debenture class not found"}
    got2 = client.get("/classes", headers=H1).json()["debenture_classes"][0]
    assert got2["debentures_outstanding"] == 600
