"""Book-accessibility regression tests for the converted share services.

Boots the two brackets that host the share capital family (the same
entry points the API gateway proxies to) and verifies every converted
service is reachable *in Book context*: records created with X-Book-ID
are stamped, visible to that Book, hidden from other Books, and still
visible in the personal (no Book) view.
"""

import importlib
import importlib.util
import os
import sys

import pytest
from fastapi.testclient import TestClient

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

USER = "book-access-user"
BOOK = "book-access-main"
OTHER_BOOK = "book-access-other"
H = {"X-User-Id": USER, "X-Book-ID": BOOK}
H_OTHER = {"X-User-Id": USER, "X-Book-ID": OTHER_BOOK}
H_PERSONAL = {"X-User-Id": USER}


def _load_bracket(name):
    path = os.path.join(REPO_ROOT, "brackets", name, "main.py")
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[name] = mod
    spec.loader.exec_module(mod)
    return mod


def _patch_fake(service_pkg, fake_name):
    fake_path = os.path.join(REPO_ROOT, service_pkg.replace("_", "-"), "fake_neo4j.py")
    spec = importlib.util.spec_from_file_location(fake_name, fake_path)
    fake = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fake)
    sys.modules[fake_name] = fake
    session = fake.FakeSession()
    db = importlib.import_module(f"{service_pkg}.database")
    db.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(session))
    return session


def _items(response, key):
    body = response.json()
    return body if isinstance(body, list) else body.get(key, [])


@pytest.fixture(scope="module")
def treasury_client():
    bracket = _load_bracket("treasury-banking-bracket")
    _patch_fake("authorized_share_capital_service", "asc_bookaccess_fake")
    _patch_fake("issued_share_capital_service", "isc_bookaccess_fake")
    _patch_fake("bank_reconciliation_service", "br_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


@pytest.fixture(scope="module")
def advanced_client():
    bracket = _load_bracket("advanced-accounting-bracket")
    _patch_fake("preference_shares_service", "psh_bookaccess_fake")
    _patch_fake("share_premium_service", "spr_bookaccess_fake")
    _patch_fake("share_redemption_service", "srd_bookaccess_fake")
    _patch_fake("ordinary_shares_service", "ord_bookaccess_fake")
    _patch_fake("bonus_shares_service", "bon_bookaccess_fake")
    _patch_fake("cashbook_service", "cb_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


# --------------------------------------------------------------------------
# Treasury-banking bracket members
# --------------------------------------------------------------------------


def test_authorized_share_capital_accessible_in_book(treasury_client):
    resp = treasury_client.post(
        "/authorized-share-capital/share-classes",
        params={"name": "Ordinary", "authorized_shares": 1000000, "par_value": 1.0, "voting_rights": "ordinary"},
        headers=H,
    )
    assert resp.status_code == 200, resp.text
    assert resp.json()["book_id"] == BOOK

    in_book = _items(treasury_client.get("/authorized-share-capital/share-classes", headers=H), "share_classes")
    assert len(in_book) == 1
    assert (
        len(_items(treasury_client.get("/authorized-share-capital/share-classes", headers=H_OTHER), "share_classes"))
        == 0
    )
    personal = _items(
        treasury_client.get("/authorized-share-capital/share-classes", headers=H_PERSONAL), "share_classes"
    )
    assert len(personal) == 1  # personal view spans Books


def test_issued_share_capital_accessible_in_book(treasury_client):
    resp = treasury_client.post(
        "/issued-share-capital/shareholders/register",
        params={"company_id": "co-1", "name": "Sam", "address": "1 Main Rd", "shares_held": 100},
        headers=H,
    )
    assert resp.status_code == 200, resp.text
    assert resp.json()["book_id"] == BOOK

    in_book = _items(treasury_client.get("/issued-share-capital/shareholders/co-1", headers=H), "shareholders")
    assert len(in_book) == 1
    assert (
        len(_items(treasury_client.get("/issued-share-capital/shareholders/co-1", headers=H_OTHER), "shareholders"))
        == 0
    )


# --------------------------------------------------------------------------
# Advanced-accounting bracket members
# --------------------------------------------------------------------------


def test_preference_shares_accessible_in_book(advanced_client):
    resp = advanced_client.post(
        "/preference-shares/classes/create",
        params={
            "name": "Series A",
            "company_id": "co-1",
            "nominal_value": 1.0,
            "issue_price": 1.5,
            "fixed_dividend_rate": 8.0,
            "dividend_type": "cumulative",
            "participation_rights": "none",
            "liquidation_priority": 1,
        },
        headers=H,
    )
    assert resp.status_code == 200, resp.text

    in_book = _items(advanced_client.get("/preference-shares/classes", headers=H), "share_classes")
    assert len(in_book) == 1
    assert len(_items(advanced_client.get("/preference-shares/classes", headers=H_OTHER), "share_classes")) == 0


def test_share_premium_accessible_in_book(advanced_client):
    resp = advanced_client.post(
        "/share-premium/entries/record",
        params={
            "company_id": "co-1",
            "entry_type": "issue",
            "shares_issued": 1000,
            "nominal_value": 1.0,
            "issue_price": 2.0,
            "share_class": "ordinary",
            "reference_id": "iss-1",
            "entry_date": "2026-09-01T10:00:00+00:00",
        },
        headers=H,
    )
    assert resp.status_code == 200, resp.text

    summary = advanced_client.get("/share-premium/summary/co-1", headers=H).json()
    assert summary["total_premium_received"] == 1000.0
    other = advanced_client.get("/share-premium/summary/co-1", headers=H_OTHER).json()
    assert other["total_premium_received"] == 0


def test_share_redemption_accessible_in_book(advanced_client):
    resp = advanced_client.post(
        "/share-redemption/redemptions/initiate",
        params={
            "company_id": "co-1",
            "share_class": "preference",
            "shares_redeemed": 100,
            "nominal_value": 1.0,
            "redemption_price": 1.2,
            "redemption_date": "2026-09-01T10:00:00+00:00",
            "redemption_method": "proceeds",
            "authority_date": "2026-08-25T10:00:00+00:00",
        },
        headers=H,
    )
    assert resp.status_code == 200, resp.text

    in_book = _items(advanced_client.get("/share-redemption/crr-requirements", headers=H), "crr_requirements")
    assert len(in_book) == 1
    assert (
        len(_items(advanced_client.get("/share-redemption/crr-requirements", headers=H_OTHER), "crr_requirements")) == 0
    )


def test_ordinary_shares_accessible_in_book(advanced_client):
    resp = advanced_client.post(
        "/ordinary-shares/dividends/declare",
        params={
            "company_id": "co-1",
            "dividend_type": "final",
            "per_share_amount": 0.10,
            "total_shares": 5000,
            "record_date": "2026-09-01T10:00:00+00:00",
        },
        headers=H,
    )
    assert resp.status_code == 200, resp.text

    in_book = _items(
        advanced_client.get("/ordinary-shares/dividends", params={"company_id": "co-1"}, headers=H), "dividends"
    )
    assert len(in_book) == 1
    assert (
        len(
            _items(
                advanced_client.get("/ordinary-shares/dividends", params={"company_id": "co-1"}, headers=H_OTHER),
                "dividends",
            )
        )
        == 0
    )


def test_bonus_shares_accessible_in_book(advanced_client):
    resp = advanced_client.post(
        "/bonus-shares/issue",
        params={
            "company_id": "co-1",
            "issue_date": "2026-09-01T10:00:00+00:00",
            "shares_issued": 1000,
            "nominal_value": 1.0,
            "source_reserve": "share_premium",
        },
        json={"holder1": 400, "holder2": 600},
        headers=H,
    )
    assert resp.status_code == 200, resp.text

    in_book = _items(advanced_client.get("/bonus-shares/issues", params={"company_id": "co-1"}, headers=H), "issues")
    assert len(in_book) == 1
    assert (
        len(
            _items(
                advanced_client.get("/bonus-shares/issues", params={"company_id": "co-1"}, headers=H_OTHER), "issues"
            )
        )
        == 0
    )


# --------------------------------------------------------------------------
# Bank reconciliation (treasury-banking bracket member)
# --------------------------------------------------------------------------


def test_bank_reconciliation_accessible_in_book(treasury_client):
    stmt = {
        "bank_account": "acc-br-access",
        "statement_number": "ST-BR-1",
        "statement_start_date": "2026-09-01T00:00:00+00:00",
        "statement_end_date": "2026-09-30T00:00:00+00:00",
        "opening_balance": 100.0,
        "closing_balance": 150.0,
        "lines": [],
    }
    resp = treasury_client.post("/bank-reconciliation/statements", json=stmt, headers=H)
    assert resp.status_code == 200, resp.text
    assert resp.json()["book_id"] == BOOK

    in_book = treasury_client.get(
        "/bank-reconciliation/statements", params={"bank_account": "acc-br-access"}, headers=H
    ).json()["statements"]
    assert len(in_book) == 1
    other = treasury_client.get(
        "/bank-reconciliation/statements", params={"bank_account": "acc-br-access"}, headers=H_OTHER
    ).json()["statements"]
    assert len(other) == 0


# --------------------------------------------------------------------------
# Cashbook (advanced-accounting bracket member)
# --------------------------------------------------------------------------


def test_cashbook_accessible_in_book(advanced_client):
    account = {"account_code": "CB-BA-1", "account_name": "Book Access Bank", "account_type": "bank"}
    resp = advanced_client.post("/cashbook/accounts", json=account, headers=H)
    assert resp.status_code == 200, resp.text
    assert resp.json()["book_id"] == BOOK

    in_book = [a for a in advanced_client.get("/cashbook/accounts", headers=H).json() if a["account_code"] == "CB-BA-1"]
    assert len(in_book) == 1
    other = [
        a for a in advanced_client.get("/cashbook/accounts", headers=H_OTHER).json() if a["account_code"] == "CB-BA-1"
    ]
    assert len(other) == 0


@pytest.fixture(scope="module")
def tax_client():
    bracket = _load_bracket("tax-audit-investigation-bracket")
    _patch_fake("tax_accounting_service", "tax_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_tax_accounting_accessible_in_book(tax_client):
    rate = {
        "tax_type": "vat",
        "jurisdiction": "ZW-BA-1",
        "jurisdiction_type": "federal",
        "rate_type": "standard",
        "rate_percentage": 15.0,
        "effective_from": "2026-01-01T00:00:00+00:00",
    }
    resp = tax_client.post("/tax-accounting/tax-rates", json=rate, headers=H)
    assert resp.status_code == 201, resp.text
    assert resp.json()["book_id"] == BOOK

    in_book = [
        r
        for r in tax_client.get("/tax-accounting/tax-rates", headers=H).json()["rates"]
        if r["jurisdiction"] == "ZW-BA-1"
    ]
    assert len(in_book) == 1
    other = [
        r
        for r in tax_client.get("/tax-accounting/tax-rates", headers=H_OTHER).json()["rates"]
        if r["jurisdiction"] == "ZW-BA-1"
    ]
    assert len(other) == 0


@pytest.fixture(scope="module")
def apar_client():
    bracket = _load_bracket("ap-ar-expenses-bracket")
    _patch_fake("benefits_admin_service", "ben_bookaccess_fake")
    _patch_fake("expense_tracking_service", "exp_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_benefits_admin_accessible_in_book(apar_client):
    resp = apar_client.post(
        "/benefits-admin/plans",
        params={"name": "Medical A", "plan_type": "medical", "employer_contribution_pct": 5.0},
        headers=H,
    )
    assert resp.status_code == 200, resp.text

    in_book = [p for p in apar_client.get("/benefits-admin/plans", headers=H).json() if p["name"] == "Medical A"]
    assert len(in_book) == 1
    other = [p for p in apar_client.get("/benefits-admin/plans", headers=H_OTHER).json() if p["name"] == "Medical A"]
    assert len(other) == 0


def test_expense_tracking_accessible_in_book(apar_client):
    expense = {
        "company_id": "co-ba-1",
        "employee_id": "emp-1",
        "category": "travel",
        "amount": 250.0,
        "description": "Book access trip",
    }
    resp = apar_client.post("/expense-tracking/expenses", json=expense, headers=H)
    assert resp.status_code == 200, resp.text

    in_book = apar_client.get("/expense-tracking/expenses/co-ba-1", headers=H).json()["expenses"]
    assert len([e for e in in_book if e["description"] == "Book access trip"]) == 1
    other = apar_client.get("/expense-tracking/expenses/co-ba-1", headers=H_OTHER).json()["expenses"]
    assert len([e for e in other if e["description"] == "Book access trip"]) == 0


@pytest.fixture(scope="module")
def document_client():
    # document-service is standalone (not a bracket member): load its app directly
    path = os.path.join(REPO_ROOT, "document-service", "main.py")
    spec = importlib.util.spec_from_file_location("document_service_main", path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules["document_service_main"] = mod
    spec.loader.exec_module(mod)
    _patch_fake("document_service", "doc_bookaccess_fake")
    with TestClient(mod.app) as client:
        yield client


def test_document_service_accessible_in_book(document_client):
    resp = document_client.post(
        "/documents",
        files={"file": ("book-access.csv", b"a,b\n1,2", "text/csv")},
        data={"title": "Book Access Doc", "document_type": "other"},
        headers=H,
    )
    assert resp.status_code == 201, resp.text
    assert resp.json()["book_id"] == BOOK

    in_book = [
        d for d in document_client.get("/documents", headers=H).json()["documents"] if d["title"] == "Book Access Doc"
    ]
    assert len(in_book) == 1
    other = [
        d
        for d in document_client.get("/documents", headers=H_OTHER).json()["documents"]
        if d["title"] == "Book Access Doc"
    ]
    assert len(other) == 0


# --------------------------------------------------------------------------
# Risk-governance bracket members (risk-assessment / risk-mitigation)
# --------------------------------------------------------------------------


@pytest.fixture(scope="module")
def risk_client():
    bracket = _load_bracket("risk-governance-bracket")
    _patch_fake("risk_assessment_service", "rasm_bookaccess_fake")
    _patch_fake("risk_mitigation_service", "rmit_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_risk_assessment_accessible_in_book(risk_client):
    payload = {
        "company_id": "co-book-access",
        "category": "financial",
        "name": "Book Access Risk",
        "likelihood": 4,
        "impact": 4,
    }
    resp = risk_client.post("/risk-assessment/risks", json=payload, headers=H)
    assert resp.status_code == 200, resp.text
    assert resp.json()["level"] == "high"

    in_book = [
        r
        for r in risk_client.get("/risk-assessment/risks/co-book-access", headers=H).json()["risks"]
        if r["name"] == "Book Access Risk"
    ]
    assert len(in_book) == 1
    other = [
        r
        for r in risk_client.get("/risk-assessment/risks/co-book-access", headers=H_OTHER).json()["risks"]
        if r["name"] == "Book Access Risk"
    ]
    assert len(other) == 0


def test_risk_mitigation_accessible_in_book(risk_client):
    payload = {
        "company_id": "co-book-access",
        "category": "operational",
        "name": "Book Access Mitigation",
        "likelihood": 2,
        "impact": 2,
    }
    resp = risk_client.post("/risk-mitigation/risks", json=payload, headers=H)
    assert resp.status_code == 200, resp.text
    assert resp.json()["level"] == "low"

    in_book = [
        r
        for r in risk_client.get("/risk-mitigation/risks/co-book-access", headers=H).json()["risks"]
        if r["name"] == "Book Access Mitigation"
    ]
    assert len(in_book) == 1
    other = [
        r
        for r in risk_client.get("/risk-mitigation/risks/co-book-access", headers=H_OTHER).json()["risks"]
        if r["name"] == "Book Access Mitigation"
    ]
    assert len(other) == 0


# --------------------------------------------------------------------------
# Risk-reporting (statements-reporting bracket) / investigation (tax-audit bracket)
# --------------------------------------------------------------------------


@pytest.fixture(scope="module")
def statements_risk_client():
    bracket = _load_bracket("statements-reporting-bracket")
    _patch_fake("risk_reporting_service", "rrpt_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_risk_reporting_accessible_in_book(statements_risk_client):
    payload = {
        "company_id": "co-book-access",
        "category": "compliance",
        "name": "Book Access Report Risk",
        "likelihood": 4,
        "impact": 4,
    }
    resp = statements_risk_client.post("/risk-reporting/risks", json=payload, headers=H)
    assert resp.status_code == 200, resp.text
    assert resp.json()["level"] == "high"

    in_book = [
        r
        for r in statements_risk_client.get("/risk-reporting/risks/co-book-access", headers=H).json()["risks"]
        if r["name"] == "Book Access Report Risk"
    ]
    assert len(in_book) == 1
    other = [
        r
        for r in statements_risk_client.get("/risk-reporting/risks/co-book-access", headers=H_OTHER).json()["risks"]
        if r["name"] == "Book Access Report Risk"
    ]
    assert len(other) == 0


@pytest.fixture(scope="module")
def investigation_client():
    bracket = _load_bracket("tax-audit-investigation-bracket")
    _patch_fake("investigation_service", "inv_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_investigation_accessible_in_book(investigation_client):
    payload = {
        "company_id": "co-book-access",
        "category": "financial",
        "name": "Book Access Investigation",
        "likelihood": 2,
        "impact": 3,
    }
    resp = investigation_client.post("/investigation/risks", json=payload, headers=H)
    assert resp.status_code == 200, resp.text
    assert resp.json()["level"] == "moderate"

    in_book = [
        r
        for r in investigation_client.get("/investigation/risks/co-book-access", headers=H).json()["risks"]
        if r["name"] == "Book Access Investigation"
    ]
    assert len(in_book) == 1
    other = [
        r
        for r in investigation_client.get("/investigation/risks/co-book-access", headers=H_OTHER).json()["risks"]
        if r["name"] == "Book Access Investigation"
    ]
    assert len(other) == 0


# --------------------------------------------------------------------------
# Balance sheet (statements-reporting bracket member)
# --------------------------------------------------------------------------

BALANCE_SHEET_PAYLOAD = {
    "company_id": "co-book-access",
    "assets": [
        {"name": "Cash", "amount": 6000.0, "category": "current", "is_liquid": True},
    ],
    "liabilities": [
        {"name": "Payables", "amount": 1000.0, "category": "current"},
    ],
    "equity": [
        {"name": "Share capital", "amount": 5000.0},
    ],
}


@pytest.fixture(scope="module")
def balance_sheet_client():
    bracket = _load_bracket("statements-reporting-bracket")
    _patch_fake("balance_sheet_service", "bs_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_balance_sheet_accessible_in_book(balance_sheet_client):
    resp = balance_sheet_client.post("/balance-sheet/generate", json=BALANCE_SHEET_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["is_balanced"] is True
    assert body["book_id"] == BOOK

    latest = balance_sheet_client.get("/balance-sheet/latest/co-book-access", headers=H).json()
    assert latest["total_assets"] == 6000.0
    assert latest["book_id"] == BOOK

    # Other Book cannot see it
    assert balance_sheet_client.get("/balance-sheet/latest/co-book-access", headers=H_OTHER).status_code == 404

    # Personal view still sees own records across Books
    personal = balance_sheet_client.get("/balance-sheet/history/co-book-access", headers=H_PERSONAL)
    assert personal.json()["total"] == 1


# --------------------------------------------------------------------------
# Cash flow statement (statements-reporting bracket member)
# --------------------------------------------------------------------------

CASH_FLOW_PAYLOAD = {
    "company_id": "co-book-access",
    "method": "direct",
    "beginning_cash": 100.0,
    "operating_activities": [
        {"description": "Customer receipts", "amount": 400.0, "is_inflow": True},
    ],
    "investing_activities": [],
    "financing_activities": [],
}


@pytest.fixture(scope="module")
def cash_flow_stmt_client():
    bracket = _load_bracket("statements-reporting-bracket")
    _patch_fake("cash_flow_statement_service", "cfs_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_cash_flow_statement_accessible_in_book(cash_flow_stmt_client):
    resp = cash_flow_stmt_client.post("/cash-flow-statement/generate", json=CASH_FLOW_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["net_change"] == 400.0
    assert body["ending_cash"] == 500.0
    assert body["book_id"] == BOOK

    latest = cash_flow_stmt_client.get("/cash-flow-statement/latest/co-book-access", headers=H).json()
    assert latest["ending_cash"] == 500.0
    assert latest["book_id"] == BOOK

    # Other Book cannot see it
    assert cash_flow_stmt_client.get("/cash-flow-statement/latest/co-book-access", headers=H_OTHER).status_code == 404

    # Personal view still sees own records across Books
    personal = cash_flow_stmt_client.get("/cash-flow-statement/history/co-book-access", headers=H_PERSONAL)
    assert personal.json()["total"] == 1


# --------------------------------------------------------------------------
# Job costing (costing-budgeting bracket member)
# --------------------------------------------------------------------------


@pytest.fixture(scope="module")
def job_costing_client():
    bracket = _load_bracket("costing-budgeting-bracket")
    _patch_fake("job_costing_service", "jc_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_job_costing_accessible_in_book(job_costing_client):
    resp = job_costing_client.post(
        "/job-costing/jobs",
        json={"company_id": "co-book-access", "job_name": "Book Access Build", "contract_value": 5000.0},
        headers=H,
    )
    assert resp.status_code == 200, resp.text
    job = resp.json()
    assert job["book_id"] == BOOK

    cost = job_costing_client.post(
        f"/job-costing/jobs/{job['id']}/costs",
        json={"cost_type": "materials", "amount": 1000.0},
        headers=H,
    )
    assert cost.status_code == 200, cost.text
    assert cost.json()["total_cost"] == 1000.0

    in_book = job_costing_client.get("/job-costing/jobs/co-book-access", headers=H).json()
    assert in_book["total"] == 1
    assert in_book["jobs"][0]["job_name"] == "Book Access Build"

    # Other Book cannot see it, and cannot add costs to it
    other = job_costing_client.get("/job-costing/jobs/co-book-access", headers=H_OTHER).json()
    assert other["total"] == 0
    blocked = job_costing_client.post(
        f"/job-costing/jobs/{job['id']}/costs",
        json={"cost_type": "labor", "amount": 10.0},
        headers=H_OTHER,
    )
    assert blocked.status_code == 404

    # Personal view still sees own records across Books
    personal = job_costing_client.get("/job-costing/jobs/co-book-access", headers=H_PERSONAL).json()
    assert personal["total"] == 1


# --------------------------------------------------------------------------
# Equity changes (advanced-accounting bracket member)
# --------------------------------------------------------------------------

EQUITY_TX_PAYLOAD = {
    "company_id": "co-book-access",
    "transaction_type": "issuance",
    "shareholder": "Book Holder",
    "shares": 100,
    "price_per_share": 10.0,
}


@pytest.fixture(scope="module")
def equity_client():
    bracket = _load_bracket("advanced-accounting-bracket")
    _patch_fake("equity_changes_service", "eq_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_equity_changes_accessible_in_book(equity_client):
    resp = equity_client.post("/equity-changes/transactions", json=EQUITY_TX_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    tx = resp.json()
    assert tx["book_id"] == BOOK
    assert tx["amount"] == 1000.0

    # Statement from those transactions
    stmt = equity_client.post(
        "/equity-changes/statement",
        json={
            "company_id": "co-book-access",
            "period": "2026-Q3",
            "beginning_equity": 5000.0,
            "transactions": [EQUITY_TX_PAYLOAD],
        },
        headers=H,
    )
    assert stmt.status_code == 200, stmt.text
    assert stmt.json()["ending_equity"] == 6000.0

    in_book = equity_client.get("/equity-changes/transactions/co-book-access", headers=H).json()
    assert in_book["total"] == 1

    # Other Book cannot see it
    other = equity_client.get("/equity-changes/transactions/co-book-access", headers=H_OTHER).json()
    assert other["total"] == 0
    other_stmts = equity_client.get("/equity-changes/statements/co-book-access", headers=H_OTHER).json()
    assert other_stmts["total"] == 0

    # Personal view still sees own records across Books
    personal = equity_client.get("/equity-changes/transactions/co-book-access", headers=H_PERSONAL).json()
    assert personal["total"] == 1


# --------------------------------------------------------------------------
# Fund accounting (advanced-accounting bracket member)
# --------------------------------------------------------------------------

FUND_PAYLOAD = {
    "company_id": "co-book-access",
    "fund_name": "Book Access Fund",
    "fund_type": "restricted",
    "balance": 400.0,
}


@pytest.fixture(scope="module")
def fund_client():
    bracket = _load_bracket("advanced-accounting-bracket")
    _patch_fake("fund_accounting_service", "fa_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_fund_accounting_accessible_in_book(fund_client):
    resp = fund_client.post("/fund-accounting/funds", json=FUND_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    fund = resp.json()
    assert fund["book_id"] == BOOK
    assert fund["net_assets"] == 400.0

    tx = fund_client.post(
        "/fund-accounting/transactions",
        json={"fund_id": fund["id"], "description": "grant income", "amount": 260.0, "is_income": True},
        headers=H,
    )
    assert tx.status_code == 200, tx.text

    in_book = fund_client.get("/fund-accounting/funds/co-book-access", headers=H).json()
    assert in_book["total_net_assets"] == 660.0
    assert fund_client.get(f"/fund-accounting/transactions/{fund['id']}", headers=H).json()["total"] == 1

    # Other Book cannot see the fund, inject transactions, or read them
    other_funds = fund_client.get("/fund-accounting/funds/co-book-access", headers=H_OTHER).json()
    assert other_funds["funds"] == []
    blocked = fund_client.post(
        "/fund-accounting/transactions",
        json={"fund_id": fund["id"], "description": "cross-book", "amount": 1.0, "is_income": True},
        headers=H_OTHER,
    )
    assert blocked.status_code == 404
    assert fund_client.get(f"/fund-accounting/transactions/{fund['id']}", headers=H_OTHER).json()["total"] == 0

    # Personal view still sees own records across Books
    personal = fund_client.get("/fund-accounting/funds/co-book-access", headers=H_PERSONAL).json()
    assert personal["total_net_assets"] == 660.0


# --------------------------------------------------------------------------
# Debt management (treasury-banking bracket member)
# --------------------------------------------------------------------------

DEBT_LOAN_PAYLOAD = {
    "company_id": "co-book-access",
    "loan_name": "Book Access Loan",
    "lender": "Stanbic",
    "principal": 100000,
    "interest_rate": 0.10,
    "term_months": 36,
    "disbursement_date": "2026-01-01",
}


@pytest.fixture(scope="module")
def debt_client():
    bracket = _load_bracket("treasury-banking-bracket")
    _patch_fake("debt_management_service", "dm_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_debt_management_accessible_in_book(debt_client):
    resp = debt_client.post("/debt-management/loans", json=DEBT_LOAN_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    loan = resp.json()
    assert loan["book_id"] == BOOK
    assert loan["remaining_balance"] == 100000.0

    in_book = debt_client.get("/debt-management/loans", params={"company_id": "co-book-access"}, headers=H).json()
    assert len(in_book) == 1

    schedule = debt_client.post(
        f"/debt-management/loans/{loan['id']}/schedule",
        params={"company_id": "co-book-access"},
        headers=H,
    )
    assert schedule.status_code == 200
    assert len(schedule.json()) == 36

    summary = debt_client.get(
        "/debt-management/summary", params={"company_id": "co-book-access", "equity": 400000}, headers=H
    ).json()
    assert summary["total_debt"] == 100000.0

    # Other Book cannot see the loan, its schedule, or the summary
    other = debt_client.get("/debt-management/loans", params={"company_id": "co-book-access"}, headers=H_OTHER).json()
    assert other == []
    assert (
        debt_client.post(
            f"/debt-management/loans/{loan['id']}/schedule",
            params={"company_id": "co-book-access"},
            headers=H_OTHER,
        ).status_code
        == 404
    )
    other_summary = debt_client.get(
        "/debt-management/summary", params={"company_id": "co-book-access"}, headers=H_OTHER
    ).json()
    assert other_summary["total_debt"] == 0

    # Personal view still sees own records across Books
    personal = debt_client.get(
        "/debt-management/loans", params={"company_id": "co-book-access"}, headers=H_PERSONAL
    ).json()
    assert len(personal) == 1


# --------------------------------------------------------------------------
# Trade finance (corporate-finance bracket member)
# --------------------------------------------------------------------------

TF_PAYLOAD = {
    "company_id": "co-book-access",
    "instrument_type": "letter_of_credit",
    "counterparty": "Overseas Supplier",
    "amount": 200000,
    "currency": "USD",
    "issuing_bank": "Stanbic",
}


@pytest.fixture(scope="module")
def trade_finance_client():
    bracket = _load_bracket("corporate-finance-bracket")
    _patch_fake("trade_finance_service", "tf_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_trade_finance_accessible_in_book(trade_finance_client):
    resp = trade_finance_client.post("/trade-finance/instruments", json=TF_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    result = resp.json()
    assert result["fee_estimate"] == 400.0
    assert result["risk_assessment"] == "medium"
    inst_id = result["id"]

    listed = trade_finance_client.get(
        "/trade-finance/instruments", params={"company_id": "co-book-access"}, headers=H
    ).json()
    assert len(listed) == 1
    assert listed[0]["book_id"] == BOOK

    presented = trade_finance_client.post(
        f"/trade-finance/instruments/{inst_id}/present", params={"company_id": "co-book-access"}, headers=H
    )
    assert presented.json()["status"] == "presented"
    settled = trade_finance_client.post(
        f"/trade-finance/instruments/{inst_id}/settle", params={"company_id": "co-book-access"}, headers=H
    )
    assert settled.json()["status"] == "paid"

    # Other Book sees nothing and cannot act on the instrument
    other = trade_finance_client.get(
        "/trade-finance/instruments", params={"company_id": "co-book-access"}, headers=H_OTHER
    ).json()
    assert other == []
    assert (
        trade_finance_client.post(
            f"/trade-finance/instruments/{inst_id}/settle",
            params={"company_id": "co-book-access"},
            headers=H_OTHER,
        ).status_code
        == 404
    )

    # Personal view still sees own records across Books
    personal = trade_finance_client.get(
        "/trade-finance/instruments", params={"company_id": "co-book-access"}, headers=H_PERSONAL
    ).json()
    assert len(personal) == 1
    assert personal[0]["status"] == "paid"
