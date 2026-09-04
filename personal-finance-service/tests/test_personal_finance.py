"""Tests for the personal finance service: recurring transactions,
debts with amortization, investments and tax estimation."""

import os
from datetime import date, timedelta

import pytest
from fastapi.testclient import TestClient

os.environ["PERSONAL_FINANCE_DB"] = "/tmp/test_personal_finance.db"
if os.path.exists(os.environ["PERSONAL_FINANCE_DB"]):
    os.remove(os.environ["PERSONAL_FINANCE_DB"])

import main  # noqa: E402

client = TestClient(main.app)


def hdr(user="user-hq"):
    return {"X-User-ID": user}


# ---------------------------------------------------------------------------
# Recurring transactions
# ---------------------------------------------------------------------------


def test_create_and_list_recurring():
    r = client.post(
        "/recurring",
        json={
            "kind": "bill",
            "description": "ZESA prepaid",
            "amount": 45,
            "frequency": "monthly",
            "next_due": date.today().isoformat(),
        },
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    rec = r.json()["recurring"]
    assert rec["kind"] == "bill"
    lst = client.get("/recurring", headers=hdr()).json()
    assert len(lst["recurring"]) == 1
    assert lst["recurring"][0]["due"] is True  # due today


def test_run_recurring_advances_due_date():
    r = client.post(
        "/recurring",
        json={
            "kind": "income",
            "description": "Salary",
            "amount": 900,
            "frequency": "monthly",
            "next_due": "2026-09-10",
        },
        headers=hdr(),
    )
    rid = r.json()["recurring"]["id"]
    run = client.post("/recurring/%s/run" % rid, headers=hdr())
    assert run.status_code == 200
    assert run.json()["next_due"] == "2026-10-10"
    recorded = run.json()["recorded"]
    assert recorded["description"] == "Salary"
    assert recorded["amount"] == 900


def test_recurring_validation():
    bad = client.post(
        "/recurring",
        json={
            "kind": "weather",
            "description": "x",
            "amount": 5,
            "frequency": "monthly",
            "next_due": "2026-09-10",
        },
        headers=hdr(),
    )
    assert bad.status_code == 400
    bad2 = client.post(
        "/recurring",
        json={
            "kind": "bill",
            "description": "x",
            "amount": 5,
            "frequency": "hourly",
            "next_due": "2026-09-10",
        },
        headers=hdr(),
    )
    assert bad2.status_code == 400


def test_recurring_requires_user_header():
    assert client.get("/recurring").status_code == 401


def test_recurring_isolated_per_user():
    r = client.post(
        "/recurring",
        json={
            "kind": "bill",
            "description": "Netflix",
            "amount": 12,
            "frequency": "monthly",
            "next_due": "2026-09-10",
        },
        headers=hdr("user-a"),
    )
    rid = r.json()["recurring"]["id"]
    # another user cannot see or run it
    assert client.get("/recurring", headers=hdr("user-b")).json()["recurring"] == []
    assert client.post("/recurring/%s/run" % rid, headers=hdr("user-b")).status_code == 404


# ---------------------------------------------------------------------------
# Debts
# ---------------------------------------------------------------------------


def make_debt(principal=10000, rate=12.0, term=24, start=None):
    start = start or date.today().isoformat()
    r = client.post(
        "/debts",
        json={
            "name": "Car loan",
            "kind": "loan",
            "principal": principal,
            "annual_rate": rate,
            "term_months": term,
            "started_at": start,
        },
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    return r.json()


def test_debt_creation_and_monthly_payment():
    j = make_debt()
    # 10k at 12%/yr for 24 months -> ~470.73/month
    assert abs(j["state"]["scheduled_monthly_payment"] - 470.73) < 0.05
    assert j["state"]["balance"] == 10000
    assert j["state"]["payments_recorded"] == 0


def test_payment_reduces_balance_with_interest():
    start = date.today()
    j = make_debt(start=start.isoformat())
    did = j["debt"]["id"]
    # day-0 payment: no interest accrued, all principal
    p1 = client.post("/debts/%s/payments" % did, json={"amount": 500}, headers=hdr())
    assert abs(p1.json()["state"]["balance"] - 9500) < 1  # ~no interest yet
    # 30 days later: interest = 9500 * 12%/365 * 30 = 93.70
    p2 = client.post(
        "/debts/%s/payments" % did,
        json={
            "amount": 500,
            "paid_at": (start + timedelta(days=30)).isoformat(),
        },
        headers=hdr(),
    )
    s = p2.json()["state"]
    assert abs(s["balance"] - 9093.70) < 1
    assert abs(s["total_interest_paid"] - 93.70) < 1
    assert s["total_paid"] == 1000


def test_debt_schedule_projection():
    j = make_debt(principal=1000, rate=0.0, term=10)
    did = j["debt"]["id"]
    sch = client.get("/debts/%s/schedule" % did, headers=hdr()).json()
    assert sch["months"] == 10
    assert sch["monthly_payment"] == 100.0
    assert sch["schedule"][-1]["balance"] == 0.0
    assert sch["total_interest_remaining"] == 0.0
    assert sch["schedule"][0]["payment"] == 100.0


def test_debt_list_totals():
    make_debt(principal=2000, rate=0.0, term=24, start=date.today().isoformat())
    lst = client.get("/debts", headers=hdr()).json()
    assert lst["total_balance"] >= 12000  # 10000 + 2000, before payments


# ---------------------------------------------------------------------------
# Investments
# ---------------------------------------------------------------------------


def make_investment(name="Old Mutual Top 200", units=10, price=5.0):
    r = client.post(
        "/investments",
        json={"name": name, "asset_class": "etf", "initial_units": units, "initial_price": price},
        headers=hdr(),
    )
    assert r.status_code == 200, r.text
    return r.json()["investment"]["id"]


def test_investment_weighted_average_cost():
    iid = make_investment(units=10, price=5.0)
    r = client.post(
        "/investments/%s/trades" % iid,
        json={"side": "buy", "units": 10, "price": 7.0},
        headers=hdr(),
    )
    inv = r.json()["investment"]
    assert inv["units"] == 20
    assert abs(inv["avg_cost"] - 6.0) < 1e-9


def test_investment_sell_and_realized_gain():
    iid = make_investment(units=10, price=5.0)
    client.post(
        "/investments/%s/trades" % iid,
        json={"side": "buy", "units": 10, "price": 7.0},
        headers=hdr(),
    )
    sell = client.post(
        "/investments/%s/trades" % iid,
        json={"side": "sell", "units": 5, "price": 9.0},
        headers=hdr(),
    )
    assert sell.json()["realized_gain"] == 15.0  # 5 * (9 - 6)
    assert sell.json()["proceeds"] == 45.0
    assert sell.json()["investment"]["units"] == 15


def test_cannot_sell_more_than_held():
    iid = make_investment(units=2, price=5.0)
    r = client.post(
        "/investments/%s/trades" % iid,
        json={"side": "sell", "units": 3, "price": 5.0},
        headers=hdr(),
    )
    assert r.status_code == 400


def test_portfolio_summary():
    iid = make_investment(name="Econet", units=10, price=5.0)
    client.post("/investments/%s/price" % iid, json={"price": 8.0}, headers=hdr())
    p = client.get("/investments", headers=hdr()).json()["portfolio"]
    # Econet: 10 units, cost 5, price 8 -> value 80, gain 30
    assert p["cost_basis"] >= 50
    assert p["unrealized_gain"] >= 30
    inv = [i for i in client.get("/investments", headers=hdr()).json()["investments"] if i["name"] == "Econet"][0]
    assert inv["gain_pct"] == 60.0


# ---------------------------------------------------------------------------
# Tax
# ---------------------------------------------------------------------------


def test_default_tax_estimate():
    r = client.post(
        "/tax/estimate",
        json={"annual_income": 50000, "deductions": 5000},
        headers=hdr(),
    )
    j = r.json()
    # taxable 45000: 20000*15% + 15000*25% = 6750
    assert j["estimated_tax"] == 6750.0
    assert j["marginal_rate"] == 0.25
    assert j["effective_rate"] == 0.135
    assert j["monthly_withholding"] == 562.5


def test_custom_brackets_replace_defaults():
    r = client.put(
        "/tax/brackets",
        json={"brackets": [{"up_to": 20000, "rate": 0.0}, {"up_to": None, "rate": 0.3}]},
        headers=hdr(),
    )
    assert r.status_code == 200
    est = client.post("/tax/estimate", json={"annual_income": 30000}, headers=hdr()).json()
    assert est["estimated_tax"] == 3000.0  # 10000 * 30%
    assert est["marginal_rate"] == 0.3


def test_brackets_validation():
    # descending rates rejected
    r = client.put(
        "/tax/brackets",
        json={"brackets": [{"up_to": None, "rate": 0.4}, {"up_to": 10000, "rate": 0.1}]},
        headers=hdr(),
    )
    assert r.status_code == 400
    # missing top band rejected
    r2 = client.put(
        "/tax/brackets",
        json={"brackets": [{"up_to": 10000, "rate": 0.1}]},
        headers=hdr(),
    )
    assert r2.status_code == 400


def test_paye_balance_due():
    j = client.post(
        "/tax/estimate",
        json={"annual_income": 50000, "deductions": 5000, "paye_paid": 6000},
        headers=hdr("tax-user"),
    ).json()
    assert j["balance_due"] == 750.0  # 6750 - 6000


# ---------------------------------------------------------------------------
# Legacy compatibility API (the original in-memory endpoints)
# ---------------------------------------------------------------------------


def test_legacy_goals_roundtrip():
    r = client.post(
        "/goals",
        json={
            "user_id": "legacy-user",
            "name": "Emergency fund",
            "target_amount": 1000,
            "current_amount": 250,
        },
    )
    assert r.status_code == 200, r.text
    g = r.json()
    assert g["name"] == "Emergency fund"
    assert g["category"] == "savings"
    lst = client.get("/goals/legacy-user").json()
    assert lst["total"] == 1
    assert abs(lst["progress_avg"] - 0.25) < 1e-9


def test_legacy_income_roundtrip():
    r = client.post(
        "/income",
        json={"user_id": "legacy-user", "source": "Salary", "amount": 800},
    )
    assert r.status_code == 200
    lst = client.get("/income/legacy-user").json()
    assert lst["total_monthly"] == 800
    assert lst["sources"][0]["frequency"] == "monthly"


def test_legacy_debt_item_still_works():
    r = client.post(
        "/debts",
        json={
            "user_id": "legacy-user",
            "creditor": "OK Zimbabwe store card",
            "balance": 300,
            "interest_rate": 24,
            "min_payment": 30,
        },
    )
    assert r.status_code == 200, r.text
    item = r.json()
    assert item["creditor"] == "OK Zimbabwe store card"
    assert item["type"] == "credit_card"
    lst = client.get("/debts/legacy-user").json()
    assert lst["total_debt"] >= 300
    assert lst["total_min_payments"] >= 30


def test_legacy_overview_aggregates_everything():
    j = client.get("/overview/legacy-user").json()
    assert j["monthly_income"] == 800
    assert j["total_debt"] >= 300
    assert j["active_goals"] == 1
    assert j["goals_target"] == 1000
    assert j["debt_to_income"] > 0


def test_new_debt_id_detail_still_resolves():
    j = make_debt(principal=500, rate=0.0, term=10)
    did = j["debt"]["id"]
    # new-style lookup with the owner header wins over legacy interpretation
    r = client.get("/debts/%s" % did, headers=hdr()).json()
    assert r["debt"]["id"] == did
    assert r["state"]["balance"] == 500
