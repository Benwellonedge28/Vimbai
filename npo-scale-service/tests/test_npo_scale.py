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


# ---------------------------------------------------------------------------
# Commercial organizations: sole trader -> enterprise
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def biz():
    r = client.post(
        "/orgs",
        json={
            "name": "Kudzai Spaza Shop",
            "org_type": "commercial",
            "annual_revenue": 8000,
            "headcount": 1,
        },
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    return r.json()["org"]["id"]


def test_sole_trader_classified(biz):
    r = client.get("/orgs/%s/features" % biz, headers=hdr())
    j = r.json()
    assert j["org_type"] == "commercial"
    assert j["size_band"] == "sole_trader"
    assert "cash_book" in j["features"]
    assert j["approval_limit"] is None


def test_sole_trader_revenue_with_receipt(biz):
    r = client.post(
        "/orgs/%s/revenues" % biz,
        json={"amount": 45.5, "source": "sale", "customer": "Tendai"},
        headers=hdr(),
    )
    assert r.status_code == 200
    assert r.json()["receipt_no"].startswith("RCP-")
    listing = client.get("/orgs/%s/revenues" % biz, headers=hdr()).json()["revenues"]
    assert listing[0]["amount"] == 45.5


def test_revenue_rejected_for_nonprofit(org):
    r = client.post(
        "/orgs/%s/revenues" % org,
        json={"amount": 10},
        headers=hdr(),
    )
    assert r.status_code == 400


def test_sole_trader_expenses_no_dual_approval(biz):
    """A sole trader approves anything alone - no bureaucracy at small scale."""
    r = client.post(
        "/orgs/%s/expenses" % biz,
        json={"amount": 250000, "fund": "general", "approver1": "kudzai"},
        headers=hdr(),
    )
    assert r.json()["status"] == "approved"


def test_enterprise_classification():
    r = client.post(
        "/orgs",
        json={
            "name": "Sable Holdings Group",
            "org_type": "commercial",
            "annual_revenue": 75_000_000,
            "headcount": 1200,
        },
        headers=hdr(),
    )
    org_id = r.json()["org"]["id"]
    f = client.get("/orgs/%s/features" % org_id, headers=hdr()).json()
    assert f["size_band"] == "extra_large"
    assert "group_consolidation" in f["features"]
    assert "intercompany" in f["features"]
    assert f["approval_limit"] == 1000


def test_business_growth_upgrades_band(biz):
    """Revenue growth moves the shop up the commercial ladder."""
    r = client.patch(
        "/orgs/%s" % biz,
        json={"annual_revenue": 200_000},
        headers=hdr(),
    )
    assert r.json()["size_band"] == "small"
    r2 = client.patch(
        "/orgs/%s" % biz,
        json={"annual_revenue": 3_000_000},
        headers=hdr(),
    )
    assert r2.json()["size_band"] == "medium"


def test_invalid_org_type_rejected():
    r = client.post(
        "/orgs",
        json={"name": "X", "org_type": "charity-shop"},
        headers=hdr(),
    )
    assert r.status_code == 400


def test_commercial_activities_report(biz):
    r = client.get("/orgs/%s/reports/activities" % biz, headers=hdr())
    funds = {f["fund"]: f for f in r.json()["funds"]}
    assert funds["sale"]["revenue"] == 45.5
    assert r.json()["total_net"] == 45.5 - 250000


# ---------------------------------------------------------------------------
# Partnerships: small firm -> international LLP
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def firm():
    r = client.post(
        "/orgs",
        json={
            "name": "Mhlanga & Dube Legal Practice",
            "org_type": "partnership",
            "annual_revenue": 250_000,
            "headcount": 6,
        },
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    return r.json()["org"]["id"]


def test_partnership_classified_small(firm):
    f = client.get("/orgs/%s/features" % firm, headers=hdr()).json()
    assert f["size_band"] == "small"
    assert f["org_type"] == "partnership"
    assert "partner_capital_accounts" in f["features"]
    assert "profit_sharing" in f["features"]


def test_partners_capital_and_profit_sharing(firm):
    p1 = client.post(
        "/orgs/%s/partners" % firm,
        json={"name": "Mai Mhlanga", "capital_contribution": 10000, "profit_share": 60},
        headers=hdr(),
    ).json()["partner_id"]
    p2 = client.post(
        "/orgs/%s/partners" % firm,
        json={"name": "Baba Dube", "capital_contribution": 5000, "profit_share": 40},
        headers=hdr(),
    ).json()["partner_id"]
    partners = client.get("/orgs/%s/partners" % firm, headers=hdr()).json()["partners"]
    assert len(partners) == 2

    # revenue in, expenses out, then a draw
    client.post(
        "/orgs/%s/revenues" % firm,
        json={"amount": 20000, "source": "service"},
        headers=hdr(),
    )
    client.post(
        "/orgs/%s/expenses" % firm,
        json={"amount": 5000, "fund": "general", "approver1": "mhangapartner"},
        headers=hdr(),
    )
    client.post(
        "/orgs/%s/partners/%s/draws" % (firm, p1),
        json={"amount": 3000},
        headers=hdr(),
    )

    # capital accounts: net income 15000 split 60/40
    r = client.get("/orgs/%s/reports/capital-accounts" % firm, headers=hdr()).json()
    assert r["net_income"] == 15000
    by_name = {a["partner"]: a for a in r["accounts"]}
    mai = by_name["Mai Mhlanga"]
    assert mai["allocated_income"] == 9000
    assert mai["draws"] == 3000
    assert mai["capital_account"] == 10000 + 9000 - 3000
    dube = by_name["Baba Dube"]
    assert dube["capital_account"] == 5000 + 6000


def test_partner_rejected_for_commercial(biz):
    r = client.post(
        "/orgs/%s/partners" % biz,
        json={"name": "Not A Partner"},
        headers=hdr(),
    )
    assert r.status_code == 400


def test_partnership_growth_to_large(firm):
    r = client.patch(
        "/orgs/%s" % firm,
        json={"annual_revenue": 20_000_000},
        headers=hdr(),
    )
    assert r.json()["size_band"] == "large"
    f = client.get("/orgs/%s/features" % firm, headers=hdr()).json()
    assert "joint_venture_accounts" not in f["features"]
    r2 = client.patch(
        "/orgs/%s" % firm,
        json={"annual_revenue": 90_000_000},
        headers=hdr(),
    )
    assert r2.json()["size_band"] == "extra_large"
    f2 = client.get("/orgs/%s/features" % firm, headers=hdr()).json()
    assert "joint_venture_accounts" in f2["features"]
    assert "group_consolidation" in f2["features"]


def test_invalid_profit_share_rejected(firm):
    r = client.post(
        "/orgs/%s/partners" % firm,
        json={"name": "Greedy", "profit_share": 150},
        headers=hdr(),
    )
    assert r.status_code == 400


def test_partnership_activities_report(firm):
    r = client.get("/orgs/%s/reports/activities" % firm, headers=hdr())
    funds = {f["fund"]: f for f in r.json()["funds"]}
    assert funds["service"]["revenue"] == 20000


# ---------------------------------------------------------------------------
# Private limited companies: small -> group holding
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def ltd():
    r = client.post(
        "/orgs",
        json={
            "name": "Tariro Engineering (Pvt) Ltd",
            "org_type": "company",
            "annual_revenue": 300_000,
            "headcount": 12,
        },
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    return r.json()["org"]["id"]


def test_company_classified_small(ltd):
    f = client.get("/orgs/%s/features" % ltd, headers=hdr()).json()
    assert f["size_band"] == "small"
    assert f["org_type"] == "company"
    assert "share_capital" in f["features"]
    assert "shareholders_register" in f["features"]


def test_shareholders_and_dividend(ltd):
    client.post(
        "/orgs/%s/shareholders" % ltd,
        json={"name": "Rudo Tariro", "shares": 6000, "amount_paid": 6000},
        headers=hdr(),
    )
    client.post(
        "/orgs/%s/shareholders" % ltd,
        json={"name": "Tapiwa Moyo", "shares": 4000, "amount_paid": 4000},
        headers=hdr(),
    )
    client.post(
        "/orgs/%s/revenues" % ltd,
        json={"amount": 30000, "source": "contract"},
        headers=hdr(),
    )
    client.post(
        "/orgs/%s/expenses" % ltd,
        json={"amount": 10000, "fund": "general", "approver1": "rudo"},
        headers=hdr(),
    )
    # declare 1.00/share: 10,000 total, within reserves of 20,000
    r = client.post(
        "/orgs/%s/dividends" % ltd,
        json={"per_share": 1.0},
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    assert r.json()["total_shares"] == 10000
    assert r.json()["total"] == 10000


def test_dividend_capped_by_reserves(ltd):
    """Corporate governance: no paying shareholders more than reserves."""
    r = client.post(
        "/orgs/%s/dividends" % ltd,
        json={"per_share": 5.0},
        headers=hdr(),
    )
    assert r.status_code == 400


def test_equity_statement(ltd):
    r = client.get("/orgs/%s/reports/equity" % ltd, headers=hdr()).json()
    assert r["share_capital"] == 10000
    assert r["retained_earnings"] == 10000  # 30k - 10k - 10k dividend
    assert r["dividends_declared"] == 10000
    assert r["total_equity"] == 20000


def test_shareholder_rejected_for_partnership(firm):
    r = client.post(
        "/orgs/%s/shareholders" % firm,
        json={"name": "X", "shares": 100},
        headers=hdr(),
    )
    assert r.status_code == 400


def test_company_growth_to_group(ltd):
    client.patch(
        "/orgs/%s" % ltd,
        json={"annual_revenue": 60_000_000},
        headers=hdr(),
    )
    f = client.get("/orgs/%s/features" % ltd, headers=hdr()).json()
    assert f["size_band"] == "extra_large"
    assert "subsidiaries" in f["features"]
    assert "group_consolidation" in f["features"]


def test_company_activities_report(ltd):
    r = client.get("/orgs/%s/reports/activities" % ltd, headers=hdr())
    funds = {x["fund"]: x for x in r.json()["funds"]}
    assert funds["contract"]["revenue"] == 30000


# ---------------------------------------------------------------------------
# Vendors, purchases, creditors (all org types)
# ---------------------------------------------------------------------------


def test_vendors_purchases_creditors(ltd):
    vid = client.post(
        "/orgs/%s/vendors" % ltd,
        json={"name": "Zimbabwe Steel Supplies", "phone": "0771 234 567"},
        headers=hdr(),
    ).json()["vendor_id"]
    client.post(
        "/orgs/%s/vendors" % ltd,
        json={"name": "Office Mart"},
        headers=hdr(),
    )
    p1 = client.post(
        "/orgs/%s/purchases" % ltd,
        json={"vendor_id": vid, "description": "steel beams", "amount": 4000},
        headers=hdr(),
    ).json()["purchase_id"]
    client.post(
        "/orgs/%s/purchases" % ltd,
        json={"vendor_id": vid, "description": "bolts", "amount": 500},
        headers=hdr(),
    )

    # creditors: unpaid only
    r = client.get("/orgs/%s/reports/creditors" % ltd, headers=hdr()).json()
    assert r["total_owed"] == 4500
    steel = [c for c in r["creditors"] if c["vendor"] == "Zimbabwe Steel Supplies"]
    assert steel[0]["owed"] == 4500

    # pay the beams: creditors drop, expense is booked
    pay = client.post("/orgs/%s/purchases/%s/pay" % (ltd, p1), headers=hdr())
    assert pay.json()["status"] == "paid"
    r2 = client.get("/orgs/%s/reports/creditors" % ltd, headers=hdr()).json()
    assert r2["total_owed"] == 500
    double_pay = client.post("/orgs/%s/purchases/%s/pay" % (ltd, p1), headers=hdr())
    assert double_pay.status_code == 400


def test_purchase_unknown_vendor_rejected(ltd):
    vendors = client.get("/orgs/%s/vendors" % ltd, headers=hdr()).json()["vendors"]
    r = client.post(
        "/orgs/%s/purchases" % ltd,
        json={"vendor_id": "no-such-vendor", "amount": 100},
        headers=hdr(),
    )
    assert r.status_code == 404
    assert len(vendors) >= 2


def test_directors_register(ltd, firm):
    d = client.post(
        "/orgs/%s/directors" % ltd,
        json={"name": "Rudo Tariro"},
        headers=hdr(),
    )
    assert d.status_code == 200
    directors = client.get("/orgs/%s/directors" % ltd, headers=hdr()).json()["directors"]
    assert directors[0]["name"] == "Rudo Tariro"
    # partnerships cannot have directors
    r = client.post(
        "/orgs/%s/directors" % firm,
        json={"name": "Not A Director"},
        headers=hdr(),
    )
    assert r.status_code == 400


def test_npo_can_buy_too(org):
    """Non-profits purchase from vendors the same way."""
    vid = client.post(
        "/orgs/%s/vendors" % org,
        json={"name": "Hardware Centre"},
        headers=hdr(),
    ).json()["vendor_id"]
    r = client.post(
        "/orgs/%s/purchases" % org,
        json={"vendor_id": vid, "description": "building materials", "amount": 200},
        headers=hdr(),
    )
    assert r.status_code == 200
    c = client.get("/orgs/%s/reports/creditors" % org, headers=hdr()).json()
    assert c["total_owed"] == 200


# ---------------------------------------------------------------------------
# Public limited companies: small cap to listed group
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def plc():
    r = client.post(
        "/orgs",
        json={
            "name": "Zamani Holdings PLC",
            "org_type": "plc",
            "annual_revenue": 300_000,
            "headcount": 25,
        },
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    return r.json()["org"]["id"]


def test_plc_classified_small(plc):
    f = client.get("/orgs/%s/features" % plc, headers=hdr()).json()
    assert f["size_band"] == "small"
    assert f["org_type"] == "plc"
    assert "public_share_registry" in f["features"]
    assert "share_capital" in f["features"]


def test_plc_shareholders_dividend_and_directors(plc):
    client.post(
        "/orgs/%s/shareholders" % plc,
        json={"name": "Public Investor A", "shares": 70000, "amount_paid": 70000},
        headers=hdr(),
    )
    client.post(
        "/orgs/%s/shareholders" % plc,
        json={"name": "Public Investor B", "shares": 30000, "amount_paid": 30000},
        headers=hdr(),
    )
    client.post(
        "/orgs/%s/directors" % plc,
        json={"name": "Chairperson Ncube"},
        headers=hdr(),
    )
    client.post(
        "/orgs/%s/revenues" % plc,
        json={"amount": 50000, "source": "listings"},
        headers=hdr(),
    )
    # 0.20/share on 100,000 shares = 20,000, within reserves of 50,000
    r = client.post(
        "/orgs/%s/dividends" % plc,
        json={"per_share": 0.20},
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    assert r.json()["total"] == 20000
    eq = client.get("/orgs/%s/reports/equity" % plc, headers=hdr()).json()
    assert eq["retained_earnings"] == 30000
    assert eq["total_equity"] == 130000


def test_plc_mandatory_audit_from_medium(plc):
    f = client.get("/orgs/%s/features" % plc, headers=hdr()).json()
    assert "mandatory_audit" not in f["features"]  # small cap
    client.patch(
        "/orgs/%s" % plc,
        json={"annual_revenue": 3_000_000},
        headers=hdr(),
    )
    f = client.get("/orgs/%s/features" % plc, headers=hdr()).json()
    assert f["size_band"] == "medium"
    assert "mandatory_audit" in f["features"]
    assert "public_disclosure" not in f["features"]
    assert "listing_compliance" not in f["features"]


def test_plc_growth_to_listed_group(plc):
    client.patch(
        "/orgs/%s" % plc,
        json={"annual_revenue": 200_000_000},
        headers=hdr(),
    )
    f = client.get("/orgs/%s/features" % plc, headers=hdr()).json()
    assert f["size_band"] == "extra_large"
    assert "listing_compliance" in f["features"]
    assert "sec_filings" in f["features"]
    assert "public_disclosure" in f["features"]


def test_partnership_still_rejected_for_shareholders(firm):
    r = client.post(
        "/orgs/%s/shareholders" % firm,
        json={"name": "X", "shares": 100},
        headers=hdr(),
    )
    assert r.status_code == 400


# ---------------------------------------------------------------------------
# PDF receipts and the public investor view
# ---------------------------------------------------------------------------


def test_receipt_pdf_download(org):
    receipts = client.get("/orgs/%s/receipts" % org, headers=hdr()).json()["receipts"]
    rcp = receipts[0]
    r = client.get("/orgs/%s/receipts/%s/pdf" % (org, rcp["id"]), headers=hdr())
    assert r.status_code == 200
    assert r.headers["content-type"].startswith("application/pdf")
    assert r.headers["content-disposition"].startswith("attachment")
    assert r.content.startswith(b"%PDF-1.4")
    assert b"endstream" in r.content and b"%%EOF" in r.content
    assert b"/receipts/verify/%s" % str(rcp["token"]).encode() in r.content


def test_public_investor_view(ltd):
    """A shareholder verifies their own holding with a code only."""
    sh = client.get("/orgs/%s/shareholders" % ltd, headers=hdr()).json()["shareholders"]
    rudo = [s for s in sh if s["name"] == "Rudo Tariro"][0]
    # shareholder created before verify codes existed: no code yet
    assert rudo["verify_code"] is not None  # fixtures create fresh rows
    v = client.get("/public/holdings/%s" % rudo["verify_code"])
    assert v.status_code == 200, v.text
    j = v.json()
    assert j["valid"] is True
    assert j["shareholder"] == "Rudo Tariro"
    assert j["shares"] == 6000
    assert j["percentage"] == 60.0  # 6000 of 10,000 shares
    # dividends from the 1.00/share declaration (ltd fixture declared one)
    assert j["total_dividends"] == 6000.0


def test_public_investor_bad_code(plc):
    r = client.get("/public/holdings/not-a-real-code")
    assert r.status_code == 404


def test_new_shareholder_gets_verify_code(plc):
    r = client.post(
        "/orgs/%s/shareholders" % plc,
        json={"name": "Minority Holder", "shares": 5000, "amount_paid": 5000},
        headers=hdr(),
    )
    assert r.status_code == 200
    code = r.json()["verify_code"]
    assert len(code) == 32
    v = client.get("/public/holdings/%s" % code).json()
    assert v["shareholder"] == "Minority Holder"
    assert v["shares"] == 5000
