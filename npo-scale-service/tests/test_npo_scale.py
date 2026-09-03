"""End-to-end tests for the Vimbai npo-scale-service.

Covers the full non-profit lifecycle from small community trust to
extra-large federation: org creation and auto-classification, branch
growth, donor CRM with automatic receipting and public verification,
recurring pledge collection, scale-aware expense approval, budgets,
compliance calendar and all four donor-grade reports.
"""

import os
import tempfile

import pytest
from fastapi.testclient import TestClient

TEST_DB = os.path.join(tempfile.mkdtemp(prefix="nposcale_"), "test.db")
os.environ["NPO_SCALE_DB"] = TEST_DB

from main import app  # noqa: E402

client = TestClient(app)

HQ = "user-hq"


def hdr(user=HQ):
    return {"X-User-ID": user}


@pytest.fixture(scope="module")
def org():
    r = client.post(
        "/orgs",
        json={
            "name": "Tariro Community Trust",
            "sector": "community",
            "annual_revenue": 20000,
            "headcount": 3,
        },
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    return r.json()["org"]["id"]


def test_health():
    r = client.get("/health")
    assert r.status_code == 200
    assert r.json()["status"] == "healthy"


def test_small_org_classified_small(org):
    r = client.get("/orgs/%s" % org, headers=hdr())
    assert r.json()["org"]["size_band"] == "small"
    assert "donor_crm" in r.json()["features"]
    assert "federation_chapters" not in r.json()["features"]


def test_donor_and_donation_with_receipt(org):
    donor_id = client.post(
        "/orgs/%s/donors" % org,
        json={"name": "Mai Chipo", "type": "individual"},
        headers=hdr(),
    ).json()["donor_id"]
    r = client.post(
        "/orgs/%s/donations" % org,
        json={
            "donor_id": donor_id,
            "amount": 150,
            "designation": "building-fund",
        },
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    receipt = r.json()["receipt"]
    assert receipt["receipt_no"].startswith("RCP-")

    # public verification works with the token
    v = client.get("/receipts/verify/%s" % receipt["token"])
    assert v.status_code == 200
    assert v.json()["valid"] is True
    assert v.json()["receipt"]["amount"] == 150

    # a bogus token is rejected
    bad = client.get("/receipts/verify/deadbeef")
    assert bad.status_code == 404


def test_small_org_expenses_single_approval(org):
    r = client.post(
        "/orgs/%s/expenses" % org,
        json={"amount": 800, "fund": "building-fund", "approver1": "treasurer"},
        headers=hdr(),
    )
    assert r.status_code == 200
    assert r.json()["status"] == "approved"


def test_consolidated_locked_for_small(org):
    r = client.get("/orgs/%s/reports/consolidated" % org, headers=hdr())
    assert r.status_code == 403


def test_growth_upgrades_band_to_large(org):
    """Branch growth pushes a small trust up to large scale."""
    for i in range(6):
        client.post(
            "/orgs/%s/branches" % org,
            json={"name": "Chapter %d" % i, "region": "Harare"},
            headers=hdr(),
        )
    r = client.get("/orgs/%s/features" % org, headers=hdr())
    band = r.json()["size_band"]
    assert band == "large"
    assert "consolidated_reporting" in r.json()["features"]


def test_dual_approval_enforced_at_large(org):
    """Above the large-band limit, expenses need two approvers."""
    r = client.post(
        "/orgs/%s/expenses" % org,
        json={"amount": 5000, "fund": "general", "approver1": "treasurer"},
        headers=hdr(),
    )
    assert r.json()["status"] == "pending_dual_approval"

    # same person twice is rejected
    r2 = client.post(
        "/orgs/%s/expenses" % org,
        json={
            "amount": 5000,
            "fund": "general",
            "approver1": "treasurer",
            "approver2": "treasurer",
        },
        headers=hdr(),
    )
    assert r2.status_code == 400

    # two different approvers passes
    r3 = client.post(
        "/orgs/%s/expenses" % org,
        json={
            "amount": 5000,
            "fund": "general",
            "approver1": "treasurer",
            "approver2": "chair",
        },
        headers=hdr(),
    )
    assert r3.json()["status"] == "approved"


def test_consolidated_report_after_growth(org):
    r = client.get("/orgs/%s/reports/consolidated" % org, headers=hdr())
    assert r.status_code == 200
    body = r.json()
    assert body["size_band"] == "large"
    assert len(body["branches"]) == 6
    assert "consolidated" in body


def test_xl_classification_by_revenue():
    r = client.post(
        "/orgs",
        json={
            "name": "Africa Relief Federation",
            "annual_revenue": 12_000_000,
            "headcount": 800,
        },
        headers=hdr(),
    )
    org_id = r.json()["org"]["id"]
    f = client.get("/orgs/%s/features" % org_id, headers=hdr())
    assert f.json()["size_band"] == "extra_large"
    assert "cross_border_consolidation" in f.json()["features"]
    assert f.json()["approval_limit"] == 1000


def test_pledge_recurring_collection(org):
    donor_id = client.post(
        "/orgs/%s/donors" % org,
        json={"name": "Monthly Giver"},
        headers=hdr(),
    ).json()["donor_id"]
    r = client.post(
        "/orgs/%s/pledges" % org,
        json={"donor_id": donor_id, "amount": 25, "frequency": "monthly"},
        headers=hdr(),
    )
    assert r.status_code == 200
    # first run: nothing due yet (next_due is 30 days out)
    run = client.post("/orgs/%s/pledges/run" % org, headers=hdr())
    assert run.json()["collected"] == 0


def test_budgets_and_reports(org):
    client.put(
        "/orgs/%s/budgets" % org,
        json={"fiscal_year": 2026, "fund": "building-fund", "budgeted": 10000},
        headers=hdr(),
    )
    # upsert updates the same line
    client.put(
        "/orgs/%s/budgets" % org,
        json={"fiscal_year": 2026, "fund": "building-fund", "budgeted": 12000},
        headers=hdr(),
    )
    r = client.get(
        "/orgs/%s/reports/budget-vs-actual" % org,
        params={"fiscal_year": 2026},
        headers=hdr(),
    )
    lines = r.json()["lines"]
    assert len(lines) == 1
    assert lines[0]["budgeted"] == 12000


def test_statement_of_activities(org):
    r = client.get("/orgs/%s/reports/activities" % org, headers=hdr())
    assert r.status_code == 200
    funds = {f["fund"]: f for f in r.json()["funds"]}
    assert "building-fund" in funds
    assert funds["building-fund"]["revenue"] == 150
    assert funds["building-fund"]["expenses"] == 800


def test_functional_expenses_and_position(org):
    client.post(
        "/orgs/%s/balance-items" % org,
        json={"kind": "asset", "name": "Bank account", "amount": 5000},
        headers=hdr(),
    )
    client.post(
        "/orgs/%s/balance-items" % org,
        json={"kind": "liability", "name": "Grill payable", "amount": 300},
        headers=hdr(),
    )
    pos = client.get("/orgs/%s/reports/position" % org, headers=hdr()).json()
    assert pos["assets"] == 5000
    assert pos["liabilities"] == 300
    func = client.get("/orgs/%s/reports/functional-expenses" % org, headers=hdr()).json()
    assert func["total"] > 0


def test_compliance_calendar(org):
    r = client.post(
        "/orgs/%s/compliance" % org,
        json={"title": "ZIMRA ITF12C", "due_date": 1767225600},
        headers=hdr(),
    )
    assert r.status_code == 200
    items = client.get("/orgs/%s/compliance" % org, headers=hdr()).json()["compliance"]
    assert any(i["title"] == "ZIMRA ITF12C" for i in items)
