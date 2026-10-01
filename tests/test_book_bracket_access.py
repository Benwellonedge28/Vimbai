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


# --------------------------------------------------------------------------
# Treasury management (treasury-banking bracket member)
# --------------------------------------------------------------------------

TM_FLOW_PAYLOAD = {
    "company_id": "co-book-access",
    "flow_type": "inflow",
    "amount": 50000,
    "currency": "USD",
    "description": "Customer payment",
}


@pytest.fixture(scope="module")
def treasury_mgmt_client():
    bracket = _load_bracket("treasury-banking-bracket")
    _patch_fake("treasury_management_service", "tmg_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_treasury_management_accessible_in_book(treasury_mgmt_client):
    resp = treasury_mgmt_client.post("/treasury-management/cashflows", json=TM_FLOW_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    assert resp.json()["status"] == "recorded"

    in_book = treasury_mgmt_client.get("/treasury-management/cashflows/co-book-access", headers=H).json()
    assert in_book["total"] == 1
    assert in_book["cashflows"][0]["book_id"] == BOOK
    assert in_book["cashflows"][0]["amount"] == 50000

    # Other Book sees nothing of the Book-A cashflows
    other = treasury_mgmt_client.get("/treasury-management/cashflows/co-book-access", headers=H_OTHER).json()
    assert other["total"] == 0

    # Position derived from the visible flows only
    pos = treasury_mgmt_client.get("/treasury-management/position/co-book-access", headers=H).json()
    assert pos["total_cash"] == 50000.0
    other_pos = treasury_mgmt_client.get("/treasury-management/position/co-book-access", headers=H_OTHER).json()
    assert other_pos["total_cash"] == 0.0

    # Position update persists within the Book and stays invisible to the other Book
    put = treasury_mgmt_client.put(
        "/treasury-management/position/co-book-access",
        json={"total_cash": 120000.0, "available_cash": 90000.0},
        headers=H,
    )
    assert put.status_code == 200, put.text
    assert put.json()["book_id"] == BOOK
    assert (
        treasury_mgmt_client.get("/treasury-management/position/co-book-access", headers=H).json()["total_cash"]
        == 120000.0
    )
    assert (
        treasury_mgmt_client.get("/treasury-management/position/co-book-access", headers=H_OTHER).json()["total_cash"]
        == 0.0
    )

    # Forecast is Book-scoped too
    fc = treasury_mgmt_client.post("/treasury-management/forecast/co-book-access", headers=H).json()
    assert fc["projected_inflows"] >= 0
    other_fc = treasury_mgmt_client.post("/treasury-management/forecast/co-book-access", headers=H_OTHER).json()
    assert other_fc["projected_inflows"] == 0

    # Personal view still sees own records across Books
    personal = treasury_mgmt_client.get("/treasury-management/cashflows/co-book-access", headers=H_PERSONAL).json()
    assert personal["total"] == 1


# --------------------------------------------------------------------------
# Revenue recognition (advanced-accounting bracket member)
# --------------------------------------------------------------------------

RR_CONTRACT_PAYLOAD = {
    "company_id": "co-book-access",
    "customer_name": "Customer A",
    "obligations": [
        {
            "description": "Software License",
            "transaction_price": 80000,
            "standalone_selling_price": 80000,
            "recognition_method": "point_in_time",
        },
        {
            "description": "Implementation",
            "transaction_price": 20000,
            "standalone_selling_price": 20000,
            "recognition_method": "over_time",
        },
    ],
}


@pytest.fixture(scope="module")
def revenue_recognition_client():
    bracket = _load_bracket("advanced-accounting-bracket")
    _patch_fake("revenue_recognition_service", "rr_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_revenue_recognition_accessible_in_book(revenue_recognition_client):
    resp = revenue_recognition_client.post("/revenue-recognition/contracts", json=RR_CONTRACT_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["total_transaction_price"] == 100000
    assert data["book_id"] == BOOK
    contract_id = data["id"]
    obligation_id = data["obligations"][0]["id"]

    # recognize within the Book
    recog = revenue_recognition_client.post(
        f"/revenue-recognition/contracts/{contract_id}/recognize",
        params={"obligation_id": obligation_id, "amount": 80000},
        headers=H,
    )
    assert recog.status_code == 200, recog.text
    assert recog.json()["is_satisfied"] is True
    assert recog.json()["contract_total_recognized"] == 80000

    # Other Book sees no contracts and cannot recognize on this one
    other = revenue_recognition_client.get("/revenue-recognition/contracts/co-book-access", headers=H_OTHER).json()
    assert other["total"] == 0
    blocked = revenue_recognition_client.post(
        f"/revenue-recognition/contracts/{contract_id}/recognize",
        params={"obligation_id": obligation_id, "amount": 1000},
        headers=H_OTHER,
    )
    assert blocked.status_code == 404

    # summary is Book-scoped
    s = revenue_recognition_client.get("/revenue-recognition/summary/co-book-access", headers=H).json()
    assert s["total_contracts"] == 1
    assert s["revenue_recognized"] == 80000
    s_other = revenue_recognition_client.get("/revenue-recognition/summary/co-book-access", headers=H_OTHER).json()
    assert s_other["total_contracts"] == 0

    # Personal view still sees own contracts across Books
    personal = revenue_recognition_client.get(
        "/revenue-recognition/contracts/co-book-access", headers=H_PERSONAL
    ).json()
    assert personal["total"] == 1
    assert personal["contracts"][0]["total_revenue_recognized"] == 80000


# --------------------------------------------------------------------------
# Subscription plans (operations-inventory bracket member)
# --------------------------------------------------------------------------

SP_PLAN_PAYLOAD = {
    "tier": "professional",
    "name": "Pro Plan",
    "price_monthly": 199,
    "features": ["Multi-company", "Advanced reporting"],
    "max_users": 50,
    "max_companies": 10,
    "api_calls_per_month": 10000,
}


@pytest.fixture(scope="module")
def subscription_plans_client():
    bracket = _load_bracket("operations-inventory-bracket")
    _patch_fake("subscription_plans_service", "sp_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_subscription_plans_accessible_in_book(subscription_plans_client):
    resp = subscription_plans_client.post("/subscription-plans/plans", json=SP_PLAN_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    plan = resp.json()
    assert plan["book_id"] == BOOK

    # plan visible in the Book, invisible to the other Book
    assert len(subscription_plans_client.get("/subscription-plans/plans", headers=H).json()) >= 1
    other = subscription_plans_client.get("/subscription-plans/plans", headers=H_OTHER).json()
    assert all(p["id"] != plan["id"] for p in other)

    # subscribe within the Book
    sub = subscription_plans_client.post(
        "/subscription-plans/subscribe",
        params={"company_id": "co-book-access", "plan_id": plan["id"], "cycle": "annual"},
        headers=H,
    )
    assert sub.status_code == 200, sub.text
    assert sub.json()["book_id"] == BOOK

    # subscription listing is Book-scoped
    subs = subscription_plans_client.get("/subscription-plans/subscriptions/co-book-access", headers=H).json()
    assert len(subs) == 1
    assert subs[0]["billing_cycle"] == "annual"
    assert (
        subscription_plans_client.get("/subscription-plans/subscriptions/co-book-access", headers=H_OTHER).json() == []
    )

    # other Book cannot subscribe to this Book's plan
    blocked = subscription_plans_client.post(
        "/subscription-plans/subscribe",
        params={"company_id": "co-book-access", "plan_id": plan["id"]},
        headers=H_OTHER,
    )
    assert blocked.status_code == 404

    # Personal view still sees own plans across Books
    personal = subscription_plans_client.get("/subscription-plans/plans", headers=H_PERSONAL).json()
    assert any(p["id"] == plan["id"] for p in personal)


# --------------------------------------------------------------------------
# Insurance claims (corporate-finance bracket member)
# --------------------------------------------------------------------------

IC_CLAIM_PAYLOAD = {
    "company_id": "co-book-access",
    "policy_number": "POL-BK-001",
    "claim_type": "property",
    "incident_date": "2026-06-15",
    "claim_amount": 50000,
    "deductible": 5000,
    "coverage_limit": 100000,
    "description": "Warehouse fire",
}


@pytest.fixture(scope="module")
def insurance_claims_client():
    bracket = _load_bracket("corporate-finance-bracket")
    _patch_fake("insurance_claims_service", "ic_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_insurance_claims_accessible_in_book(insurance_claims_client):
    resp = insurance_claims_client.post("/insurance-claims/file", json=IC_CLAIM_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    claim = resp.json()
    assert claim["book_id"] == BOOK
    assert claim["status"] == "filed"

    # claim visible in the Book, invisible to the other Book
    mine = insurance_claims_client.get(
        "/insurance-claims/claims", params={"company_id": "co-book-access"}, headers=H
    ).json()
    assert any(c["id"] == claim["id"] for c in mine)
    other = insurance_claims_client.get(
        "/insurance-claims/claims", params={"company_id": "co-book-access"}, headers=H_OTHER
    ).json()
    assert all(c["id"] != claim["id"] for c in other)

    # process within the Book
    result = insurance_claims_client.post(
        f"/insurance-claims/claims/{claim['id']}/process",
        params={"company_id": "co-book-access"},
        headers=H,
    )
    assert result.status_code == 200, result.text
    assert result.json()["covered_amount"] == 45000
    assert result.json()["status"] == "approved"

    # other Book cannot process the claim
    blocked = insurance_claims_client.post(
        f"/insurance-claims/claims/{claim['id']}/process",
        params={"company_id": "co-book-access"},
        headers=H_OTHER,
    )
    assert blocked.status_code == 404

    # Personal view still sees own claims across Books
    personal = insurance_claims_client.get(
        "/insurance-claims/claims", params={"company_id": "co-book-access"}, headers=H_PERSONAL
    ).json()
    assert any(c["id"] == claim["id"] for c in personal)
    assert personal[[c["id"] for c in personal].index(claim["id"])]["status"] == "approved"


# --------------------------------------------------------------------------
# Sovereign treasury (treasury-banking bracket member)
# --------------------------------------------------------------------------

ST_ACCOUNT_PAYLOAD = {
    "country": "ZW-BA",
    "account_type": "stabilization_fund",
    "balance": 100000,
    "currency": "USD",
    "description": "Book-access test fund",
}


@pytest.fixture(scope="module")
def sovereign_treasury_client():
    bracket = _load_bracket("treasury-banking-bracket")
    _patch_fake("sovereign_treasury_service", "st_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_sovereign_treasury_accessible_in_book(sovereign_treasury_client):
    resp = sovereign_treasury_client.post("/sovereign-treasury/accounts", json=ST_ACCOUNT_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    account_id = resp.json()["id"]

    # account visible in the Book, invisible to the other Book
    mine = sovereign_treasury_client.get("/sovereign-treasury/accounts/ZW-BA", headers=H).json()
    assert any(a["id"] == account_id for a in mine["accounts"])
    assert mine["total_balance"] == 100000
    other = sovereign_treasury_client.get("/sovereign-treasury/accounts/ZW-BA", headers=H_OTHER).json()
    assert all(a["id"] != account_id for a in other["accounts"])
    assert other["total_balance"] == 0

    # debt registered in the Book stays invisible to the other Book
    debt = sovereign_treasury_client.post(
        "/sovereign-treasury/debt",
        json={
            "country": "ZW-BA",
            "instrument": "eurobond",
            "principal": 500000,
            "interest_rate": 7.5,
            "maturity_date": "2030-06-30T00:00:00Z",
            "outstanding": 480000,
            "currency": "USD",
        },
        headers=H,
    )
    assert debt.status_code == 200, debt.text
    debts = sovereign_treasury_client.get("/sovereign-treasury/debt/ZW-BA", headers=H).json()
    assert debts["total_debt"] == 480000
    other_debts = sovereign_treasury_client.get("/sovereign-treasury/debt/ZW-BA", headers=H_OTHER).json()
    assert other_debts["total_debt"] == 0

    # fiscal position set in the Book is Book-gated
    pos = sovereign_treasury_client.post(
        "/sovereign-treasury/fiscal-position",
        json={
            "country": "ZW-BA",
            "fiscal_year": "2026",
            "total_revenue": 4000000,
            "total_expenditure": 4500000,
            "foreign_reserves": 800000,
        },
        headers=H,
    )
    assert pos.status_code == 200, pos.text
    assert pos.json()["fiscal_deficit"] == 500000
    assert (
        sovereign_treasury_client.get("/sovereign-treasury/fiscal-position/ZW-BA", headers=H_OTHER).status_code == 404
    )
    assert sovereign_treasury_client.get("/sovereign-treasury/fiscal-position/ZW-BA", headers=H).status_code == 200

    # Personal view still sees own records across Books
    personal = sovereign_treasury_client.get("/sovereign-treasury/accounts/ZW-BA", headers=H_PERSONAL).json()
    assert any(a["id"] == account_id for a in personal["accounts"])
    assert (
        sovereign_treasury_client.get("/sovereign-treasury/fiscal-position/ZW-BA", headers=H_PERSONAL).status_code
        == 200
    )


# --------------------------------------------------------------------------
# Tax audit (tax-audit-investigation bracket member)
# --------------------------------------------------------------------------

TA_ENGAGEMENT_PAYLOAD = {
    "company_id": "co-book-access",
    "audit_type": "tax",
    "title": "Book-Access Tax Audit 2026",
    "objectives": ["Verify VAT filings"],
}


@pytest.fixture(scope="module")
def tax_audit_client():
    bracket = _load_bracket("tax-audit-investigation-bracket")
    _patch_fake("tax_audit_service", "ta_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_tax_audit_accessible_in_book(tax_audit_client):
    resp = tax_audit_client.post("/tax-audit/engagements", json=TA_ENGAGEMENT_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    eng = resp.json()
    assert eng["book_id"] == BOOK
    assert eng["status"] == "planned"

    # engagement visible in the Book, invisible to the other Book
    mine = tax_audit_client.get("/tax-audit/engagements/co-book-access", headers=H).json()
    assert any(e["id"] == eng["id"] for e in mine["engagements"])
    other = tax_audit_client.get("/tax-audit/engagements/co-book-access", headers=H_OTHER).json()
    assert all(e["id"] != eng["id"] for e in other["engagements"])

    # add a finding within the Book
    add = tax_audit_client.post(
        f"/tax-audit/engagements/{eng['id']}/findings",
        json={"title": "Under-reported income", "severity": "high", "description": "Income gap"},
        headers=H,
    )
    assert add.status_code == 200, add.text
    finding_id = add.json()["finding_id"]

    # other Book cannot add findings or complete the engagement
    blocked_add = tax_audit_client.post(
        f"/tax-audit/engagements/{eng['id']}/findings",
        json={"title": "x", "description": "y"},
        headers=H_OTHER,
    )
    assert blocked_add.status_code == 404
    blocked_status = tax_audit_client.put(
        f"/tax-audit/engagements/{eng['id']}/status", params={"status": "completed"}, headers=H_OTHER
    )
    assert blocked_status.status_code == 404

    # complete within the Book, then the report reflects it
    done = tax_audit_client.put(
        f"/tax-audit/engagements/{eng['id']}/status",
        params={"status": "completed", "summary": "Closed with one high finding"},
        headers=H,
    )
    assert done.status_code == 200, done.text
    report = tax_audit_client.get(f"/tax-audit/report/{eng['id']}", headers=H).json()
    assert report["engagement"]["status"] == "completed"
    assert report["findings_summary"]["high"] == 1
    assert report["findings_summary"]["total"] == 1
    assert tax_audit_client.get(f"/tax-audit/report/{eng['id']}", headers=H_OTHER).status_code == 404

    # remediate within the Book
    rem = tax_audit_client.put(
        f"/tax-audit/findings/{finding_id}/remediate",
        params={"remediation_note": "Amended return filed"},
        headers=H,
    )
    assert rem.status_code == 200, rem.text
    assert rem.json()["status"] == "remediated"

    # Personal view still sees own engagements across Books
    personal = tax_audit_client.get("/tax-audit/engagements/co-book-access", headers=H_PERSONAL).json()
    assert any(e["id"] == eng["id"] for e in personal["engagements"])


# --------------------------------------------------------------------------
# Appropriation control (costing-budgeting bracket member)
# --------------------------------------------------------------------------

AC_APPROPRIATION_PAYLOAD = {
    "company_id": "co-book-access",
    "department": "IT",
    "fiscal_year": "2026",
    "approved_amount": 100000,
}


@pytest.fixture(scope="module")
def appropriation_client():
    bracket = _load_bracket("costing-budgeting-bracket")
    _patch_fake("appropriation_control_service", "ac_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_appropriation_control_accessible_in_book(appropriation_client):
    resp = appropriation_client.post("/appropriation-control/appropriations", json=AC_APPROPRIATION_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    appr = resp.json()
    assert appr["book_id"] == BOOK
    assert appr["available_amount"] == 100000

    # appropriation visible in the Book, invisible to the other Book
    mine = appropriation_client.get("/appropriation-control/appropriations/co-book-access", headers=H).json()
    assert any(a["id"] == appr["id"] for a in mine["appropriations"])
    other = appropriation_client.get("/appropriation-control/appropriations/co-book-access", headers=H_OTHER).json()
    assert all(a["id"] != appr["id"] for a in other["appropriations"])

    # transact within the Book
    commit = appropriation_client.post(
        "/appropriation-control/transactions",
        json={"appropriation_id": appr["id"], "type": "commit", "amount": 30000},
        headers=H,
    )
    assert commit.status_code == 200, commit.text
    assert commit.json()["available"] == 70000

    # other Book cannot transact or check
    blocked_tx = appropriation_client.post(
        "/appropriation-control/transactions",
        json={"appropriation_id": appr["id"], "type": "spend", "amount": 30000},
        headers=H_OTHER,
    )
    assert blocked_tx.status_code == 404
    blocked_check = appropriation_client.get(
        f"/appropriation-control/check/{appr['id']}", params={"amount": 1000}, headers=H_OTHER
    )
    assert blocked_check.status_code == 404

    # spend within the Book, availability reflects it
    spend = appropriation_client.post(
        "/appropriation-control/transactions",
        json={"appropriation_id": appr["id"], "type": "spend", "amount": 30000},
        headers=H,
    )
    assert spend.json()["available"] == 70000
    check = appropriation_client.get(
        f"/appropriation-control/check/{appr['id']}", params={"amount": 80000}, headers=H
    ).json()
    assert check["allowed"] is False

    # Personal view still sees own appropriations across Books
    personal = appropriation_client.get(
        "/appropriation-control/appropriations/co-book-access", headers=H_PERSONAL
    ).json()
    assert any(a["id"] == appr["id"] for a in personal["appropriations"])
    idx = [a["id"] for a in personal["appropriations"]].index(appr["id"])
    assert personal["appropriations"][idx]["available_amount"] == 70000


# --------------------------------------------------------------------------
# Bank relationship (treasury-banking bracket member)
# --------------------------------------------------------------------------

BR_RELATIONSHIP_PAYLOAD = {
    "company_id": "co-book-access",
    "bank_name": "CBZ Bank",
    "services": ["checking", "credit_line"],
    "rating": 4,
}


@pytest.fixture(scope="module")
def bank_relationship_client():
    bracket = _load_bracket("treasury-banking-bracket")
    _patch_fake("bank_relationship_service", "br_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_bank_relationship_accessible_in_book(bank_relationship_client):
    resp = bank_relationship_client.post("/bank-relationship/relationships", json=BR_RELATIONSHIP_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    rel = resp.json()
    assert rel["book_id"] == BOOK
    assert rel["bank_name"] == "CBZ Bank"

    # relationship visible in the Book, invisible to the other Book
    mine = bank_relationship_client.get("/bank-relationship/relationships/co-book-access", headers=H).json()
    assert any(r["id"] == rel["id"] for r in mine["relationships"])
    other = bank_relationship_client.get("/bank-relationship/relationships/co-book-access", headers=H_OTHER).json()
    assert all(r["id"] != rel["id"] for r in other["relationships"])

    # update within the Book persists
    upd = bank_relationship_client.put(f"/bank-relationship/relationships/{rel['id']}", params={"rating": 5}, headers=H)
    assert upd.status_code == 200, upd.text
    assert upd.json()["rating"] == 5

    # other Book cannot update
    blocked = bank_relationship_client.put(
        f"/bank-relationship/relationships/{rel['id']}", params={"rating": 1}, headers=H_OTHER
    )
    assert blocked.status_code == 404

    # quality metrics attach within the Book only
    add = bank_relationship_client.post(
        "/bank-relationship/quality-metrics",
        json={"relationship_id": rel["id"], "metric_name": "response_time", "score": 4},
        headers=H,
    )
    assert add.status_code == 200, add.text
    blocked_metric = bank_relationship_client.post(
        "/bank-relationship/quality-metrics",
        json={"relationship_id": rel["id"], "metric_name": "x", "score": 1},
        headers=H_OTHER,
    )
    assert blocked_metric.status_code == 404
    metrics = bank_relationship_client.get(f"/bank-relationship/quality-metrics/{rel['id']}", headers=H).json()
    assert metrics["avg_score"] == 4.0
    assert (
        bank_relationship_client.get(f"/bank-relationship/quality-metrics/{rel['id']}", headers=H_OTHER).status_code
        == 404
    )

    # summary reflects the Book's relationships only
    summary = bank_relationship_client.get("/bank-relationship/summary/co-book-access", headers=H).json()
    assert summary["total_relationships"] == 1
    other_summary = bank_relationship_client.get("/bank-relationship/summary/co-book-access", headers=H_OTHER).json()
    assert other_summary["total_relationships"] == 0

    # Personal view still sees own relationships across Books
    personal = bank_relationship_client.get(
        "/bank-relationship/relationships/co-book-access", headers=H_PERSONAL
    ).json()
    assert any(r["id"] == rel["id"] for r in personal["relationships"])


# --------------------------------------------------------------------------
# Cash optimization (corporate-finance bracket member)
# --------------------------------------------------------------------------

CO_ACCOUNTS = [
    {
        "company_id": "co-book-access",
        "account_name": "Operating",
        "account_type": "operating",
        "balance": 200000,
        "min_required": 50000,
    },
    {
        "company_id": "co-book-access",
        "account_name": "Investment",
        "account_type": "investment",
        "balance": 50000,
        "interest_rate": 0.05,
    },
]


@pytest.fixture(scope="module")
def cash_opt_client():
    bracket = _load_bracket("corporate-finance-bracket")
    _patch_fake("cash_optimization_service", "cashopt_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_cash_optimization_accessible_in_book(cash_opt_client):
    for acct in CO_ACCOUNTS:
        resp = cash_opt_client.post("/cash-optimization/accounts", json=acct, headers=H)
        assert resp.status_code == 200, resp.text

    # accounts visible in the Book, invisible to the other Book
    mine = cash_opt_client.get("/cash-optimization/accounts/co-book-access", headers=H).json()
    assert len(mine["accounts"]) == 2
    other = cash_opt_client.get("/cash-optimization/accounts/co-book-access", headers=H_OTHER).json()
    assert other["accounts"] == []

    # optimize within the Book
    resp = cash_opt_client.post("/cash-optimization/optimize/co-book-access", headers=H)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["total_count"] >= 1
    assert data["potential_annual_benefit"] > 0

    # suggestions persist and stay Book-scoped
    stored = cash_opt_client.get("/cash-optimization/suggestions/co-book-access", headers=H).json()
    assert len(stored["suggestions"]) == data["total_count"]
    other_stored = cash_opt_client.get("/cash-optimization/suggestions/co-book-access", headers=H_OTHER).json()
    assert other_stored["suggestions"] == []

    # other Book's optimize run sees no accounts and produces nothing
    other_run = cash_opt_client.post("/cash-optimization/optimize/co-book-access", headers=H_OTHER).json()
    assert other_run["total_count"] == 0
    # the Book's own suggestions survive the other Book's run untouched
    still_there = cash_opt_client.get("/cash-optimization/suggestions/co-book-access", headers=H).json()
    assert len(still_there["suggestions"]) == data["total_count"]

    # Personal view still sees own accounts across Books
    personal = cash_opt_client.get("/cash-optimization/accounts/co-book-access", headers=H_PERSONAL).json()
    assert len(personal["accounts"]) == 2


# --------------------------------------------------------------------------
# Process + product costing (costing-budgeting bracket members, twins)
# --------------------------------------------------------------------------

COSTING_COMPONENTS = [
    {"name": "Raw materials", "amount": 5000, "cost_type": "direct_materials"},
    {"name": "Labour", "amount": 3000, "cost_type": "direct_labor"},
    {"name": "Overhead", "amount": 2000, "cost_type": "overhead"},
]

COSTING_PAYLOAD = {
    "company_id": "co-book-access",
    "product_or_process": "Widget Assembly",
    "period": "2026-Q1",
    "quantity": 100,
    "components": COSTING_COMPONENTS,
}


def _costing_flow(client, prefix):
    resp = client.post(f"/{prefix}/calculate", json=COSTING_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    calc = resp.json()
    assert calc["book_id"] == BOOK
    assert calc["total_cost"] == 10000.0
    assert calc["unit_cost"] == 100.0

    # calculation visible in the Book, invisible to the other Book
    mine = client.get(f"/{prefix}/calculations/co-book-access", headers=H).json()
    assert any(c["id"] == calc["id"] for c in mine["calculations"])
    other = client.get(f"/{prefix}/calculations/co-book-access", headers=H_OTHER).json()
    assert all(c["id"] != calc["id"] for c in other["calculations"])

    # breakdown within the Book; cross-scope 404
    bd = client.get(f"/{prefix}/breakdown/co-book-access/{calc['id']}", headers=H).json()
    assert bd["breakdown"]["direct_materials"] == 5000.0
    assert client.get(f"/{prefix}/breakdown/co-book-access/{calc['id']}", headers=H_OTHER).status_code == 404

    # summary reflects the Book's calculations only
    summary = client.get(f"/{prefix}/summary/co-book-access", headers=H).json()
    assert summary["total_calculations"] >= 1
    other_summary = client.get(f"/{prefix}/summary/co-book-access", headers=H_OTHER).json()
    assert other_summary["total_calculations"] == 0

    # Personal view still sees own calculations across Books
    personal = client.get(f"/{prefix}/calculations/co-book-access", headers=H_PERSONAL).json()
    assert any(c["id"] == calc["id"] for c in personal["calculations"])


@pytest.fixture(scope="module")
def process_costing_client():
    bracket = _load_bracket("costing-budgeting-bracket")
    _patch_fake("process_costing_service", "pc_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


@pytest.fixture(scope="module")
def product_costing_client():
    bracket = _load_bracket("costing-budgeting-bracket")
    _patch_fake("product_costing_service", "prc_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_process_costing_accessible_in_book(process_costing_client):
    _costing_flow(process_costing_client, "process-costing")


def test_product_costing_accessible_in_book(product_costing_client):
    _costing_flow(product_costing_client, "product-costing")


# --------------------------------------------------------------------------
# Scenario + sensitivity analysis (corporate-finance bracket members)
# --------------------------------------------------------------------------

SCENARIO_ANALYZE_PAYLOAD = {
    "company_id": "co-book-access",
    "base_revenue": 1000000,
    "base_cost": 700000,
    "base_interest": 20000,
    "base_depreciation": 50000,
    "best_case": {"revenue_growth": 0.2, "cost_growth": 0.03, "description": "Optimistic"},
    "base_case": {"revenue_growth": 0.1, "cost_growth": 0.05, "description": "Expected"},
    "worst_case": {"revenue_growth": -0.1, "cost_growth": 0.08, "description": "Pessimistic"},
}

SENSITIVITY_ANALYZE_PAYLOAD = {
    "company_id": "co-book-access",
    "target_metric": "net_profit",
    "base_target_value": 100000,
    "variables": [
        {"name": "revenue", "base_value": 500000, "change_pct": 10},
        {"name": "costs", "base_value": 400000, "change_pct": 10},
    ],
    "change_steps": [-10, 0, 10],
}


@pytest.fixture(scope="module")
def scenario_analysis_client():
    bracket = _load_bracket("corporate-finance-bracket")
    _patch_fake("scenario_analysis_service", "scen_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


@pytest.fixture(scope="module")
def sensitivity_analysis_client():
    bracket = _load_bracket("corporate-finance-bracket")
    _patch_fake("sensitivity_analysis_service", "sens_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_scenario_analysis_accessible_in_book(scenario_analysis_client):
    # the /analyze modeling itself is stateless computation
    resp = scenario_analysis_client.post("/scenario-analysis/analyze", json=SCENARIO_ANALYZE_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["best_case"]["net_income"] > data["worst_case"]["net_income"]
    assert len(data["recommendation"]) > 0

    # stored scenarios are Book-scoped
    created = scenario_analysis_client.post(
        "/scenario-analysis/scenarios",
        json={
            "company_id": "co-book-access",
            "name": "Base",
            "projected_revenue": 150000,
            "projected_expenses": 100000,
        },
        headers=H,
    )
    assert created.status_code == 200, created.text
    scenario = created.json()
    assert scenario["book_id"] == BOOK

    mine = scenario_analysis_client.get("/scenario-analysis/scenarios/co-book-access", headers=H).json()
    assert any(s["id"] == scenario["id"] for s in mine["scenarios"])
    other = scenario_analysis_client.get("/scenario-analysis/scenarios/co-book-access", headers=H_OTHER).json()
    assert all(s["id"] != scenario["id"] for s in other["scenarios"])

    # comparison only over the Book's own scenarios
    scenario_analysis_client.post(
        "/scenario-analysis/scenarios",
        json={
            "company_id": "co-book-access",
            "name": "Bull",
            "projected_revenue": 300000,
            "projected_expenses": 100000,
        },
        headers=H,
    )
    cmp_data = scenario_analysis_client.get("/scenario-analysis/compare/co-book-access", headers=H).json()
    assert cmp_data["best_case"] == "Bull"
    assert cmp_data["worst_case"] == "Base"
    other_cmp = scenario_analysis_client.get("/scenario-analysis/compare/co-book-access", headers=H_OTHER).json()
    assert other_cmp["comparison"] == "Need at least 2 scenarios"

    # Personal view still sees own scenarios across Books
    personal = scenario_analysis_client.get("/scenario-analysis/scenarios/co-book-access", headers=H_PERSONAL).json()
    assert any(s["id"] == scenario["id"] for s in personal["scenarios"])


def test_sensitivity_analysis_accessible_in_book(sensitivity_analysis_client):
    resp = sensitivity_analysis_client.post(
        "/sensitivity-analysis/analyze", json=SENSITIVITY_ANALYZE_PAYLOAD, headers=H
    )
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert len(data["results"]) == 6
    assert data["book_id"] == BOOK

    # analysis persisted and visible in the Book only
    mine = sensitivity_analysis_client.get("/sensitivity-analysis/analyses/co-book-access", headers=H).json()
    assert mine["total"] == 1
    other = sensitivity_analysis_client.get("/sensitivity-analysis/analyses/co-book-access", headers=H_OTHER).json()
    assert other["total"] == 0

    # Personal view still sees own analyses across Books
    personal = sensitivity_analysis_client.get(
        "/sensitivity-analysis/analyses/co-book-access", headers=H_PERSONAL
    ).json()
    assert personal["total"] == 1


# --------------------------------------------------------------------------
# Zero-based budgeting (costing-budgeting bracket member)
# --------------------------------------------------------------------------

ZBB_PACKAGE_PAYLOAD = {
    "company_id": "co-book-access",
    "period": "2026-Q1",
    "name": "IT Budget",
    "department": "IT",
    "items": [
        {
            "department": "IT",
            "category": "software",
            "description": "Licenses",
            "amount": 50000,
            "justification": "Required for ops",
        }
    ],
}


@pytest.fixture(scope="module")
def zbb_book_client():
    bracket = _load_bracket("costing-budgeting-bracket")
    _patch_fake("zero_based_budgeting_service", "zbb_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_zero_based_budgeting_accessible_in_book(zbb_book_client):
    resp = zbb_book_client.post("/zero-based-budgeting/packages", json=ZBB_PACKAGE_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    pkg = resp.json()
    assert pkg["book_id"] == BOOK
    assert pkg["total_amount"] == 50000.0

    # visible in the Book, invisible to the other Book
    mine = zbb_book_client.get("/zero-based-budgeting/packages/co-book-access", headers=H).json()
    assert any(p["id"] == pkg["id"] for p in mine["packages"])
    other = zbb_book_client.get("/zero-based-budgeting/packages/co-book-access", headers=H_OTHER).json()
    assert all(p["id"] != pkg["id"] for p in other["packages"])

    # workflow operations stay Book-scoped; cross-Book status update 404
    add = zbb_book_client.post(
        f"/zero-based-budgeting/packages/{pkg['id']}/items",
        json={
            "department": "IT",
            "category": "hardware",
            "description": "Laptops",
            "amount": 20000,
            "justification": "New hires",
        },
        headers=H,
    )
    assert add.status_code == 200, add.text
    assert add.json()["total_amount"] == 70000.0
    assert (
        zbb_book_client.put(
            f"/zero-based-budgeting/packages/{pkg['id']}/status", params={"status": "approved"}, headers=H_OTHER
        ).status_code
        == 404
    )
    approved = zbb_book_client.put(
        f"/zero-based-budgeting/packages/{pkg['id']}/status", params={"status": "approved"}, headers=H
    )
    assert approved.status_code == 200

    # summary reflects the Book's packages only
    summary = zbb_book_client.get("/zero-based-budgeting/summary/co-book-access", headers=H).json()
    assert summary["total_packages"] == 1
    assert summary["total_budget"] == 70000.0
    assert summary["by_status"] == {"approved": 1}
    other_summary = zbb_book_client.get("/zero-based-budgeting/summary/co-book-access", headers=H_OTHER).json()
    assert other_summary["total_packages"] == 0

    # Personal view still sees own packages across Books
    personal = zbb_book_client.get("/zero-based-budgeting/packages/co-book-access", headers=H_PERSONAL).json()
    assert any(p["id"] == pkg["id"] for p in personal["packages"])


# --------------------------------------------------------------------------
# Webhook service (platform-automation bracket member)
# --------------------------------------------------------------------------


@pytest.fixture(scope="module")
def webhook_book_client():
    bracket = _load_bracket("platform-automation-bracket")
    _patch_fake("webhook_service", "webhook_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_webhook_accessible_in_book(webhook_book_client):
    created = webhook_book_client.post(
        "/webhook/endpoints",
        json={"company_id": "co-book-access", "url": "https://example.com/hook", "events": ["invoice.created"]},
        headers=H,
    )
    assert created.status_code == 200, created.text
    ep = created.json()

    mine = webhook_book_client.get("/webhook/endpoints/co-book-access", headers=H).json()
    assert mine["total"] == 1
    other = webhook_book_client.get("/webhook/endpoints/co-book-access", headers=H_OTHER).json()
    assert other["total"] == 0

    # Personal view still sees own endpoints across Books
    personal = webhook_book_client.get("/webhook/endpoints/co-book-access", headers=H_PERSONAL).json()
    assert personal["total"] == 1

    # cross-Book dispatch must not reach this endpoint (network is mocked by
    # the TestClient transport failing; only scoping matters here)
    resp = webhook_book_client.post(
        "/webhook/dispatch/co-book-access", params={"event_type": "invoice.created"}, json={"x": 1}, headers=H_OTHER
    )
    assert resp.status_code == 200, resp.text
    assert resp.json()["total_sent"] == 0


# --------------------------------------------------------------------------
# Policy engine (platform-infrastructure bracket member)
# --------------------------------------------------------------------------

POLICY_RULE_PAYLOAD = {
    "name": "Large Transaction Check",
    "resource_type": "transaction",
    "condition_field": "amount",
    "condition_operator": ">",
    "condition_value": 50000,
    "action": "deny",
    "message": "Transaction requires approval",
}


@pytest.fixture(scope="module")
def policy_book_client():
    bracket = _load_bracket("platform-infrastructure-bracket")
    _patch_fake("policy_engine_service", "policy_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_policy_engine_accessible_in_book(policy_book_client):
    created = policy_book_client.post("/policy/rules/co-book-access", json=POLICY_RULE_PAYLOAD, headers=H)
    assert created.status_code == 200, created.text
    rule = created.json()

    mine = policy_book_client.get("/policy/rules/co-book-access", headers=H).json()
    assert mine["total"] == 1
    other = policy_book_client.get("/policy/rules/co-book-access", headers=H_OTHER).json()
    assert other["total"] == 0

    # Personal view still sees own rules across Books
    personal = policy_book_client.get("/policy/rules/co-book-access", headers=H_PERSONAL).json()
    assert personal["total"] == 1

    # evaluation applies only the Book's rules
    result = policy_book_client.post(
        "/policy/evaluate/co-book-access",
        params={"resource_type": "transaction"},
        json={"amount": 80000},
        headers=H,
    ).json()
    assert result["triggered_count"] == 1
    assert result["blocked"] is True
    other_result = policy_book_client.post(
        "/policy/evaluate/co-book-access",
        params={"resource_type": "transaction"},
        json={"amount": 80000},
        headers=H_OTHER,
    ).json()
    assert other_result["triggered_count"] == 0
    assert other_result["allowed"] is True


# --------------------------------------------------------------------------
# Financial identity (advanced-accounting bracket member)
# --------------------------------------------------------------------------

KYC_PROFILE_PAYLOAD = {"user_id": "subject-book-access", "legal_name": "Book Access Subject"}


@pytest.fixture(scope="module")
def identity_book_client():
    bracket = _load_bracket("advanced-accounting-bracket")
    _patch_fake("financial_identity_service", "fin_identity_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_financial_identity_accessible_in_book(identity_book_client):
    created = identity_book_client.post("/financial-identity/profiles", json=KYC_PROFILE_PAYLOAD, headers=H)
    assert created.status_code == 200, created.text
    profile = created.json()
    assert profile["verification_status"] == "pending"

    mine = identity_book_client.get(f"/financial-identity/profiles/{profile['id']}", headers=H).json()
    assert mine["legal_name"] == "Book Access Subject"
    # other Book cannot read or verify this KYC profile
    assert identity_book_client.get(f"/financial-identity/profiles/{profile['id']}", headers=H_OTHER).status_code == 404
    assert (
        identity_book_client.put(
            f"/financial-identity/profiles/{profile['id']}/verify", json=["passport", "utility_bill"], headers=H_OTHER
        ).status_code
        == 404
    )

    # verification within the Book works and persists
    verified = identity_book_client.put(
        f"/financial-identity/profiles/{profile['id']}/verify", json=["passport", "utility_bill"], headers=H
    )
    assert verified.status_code == 200, verified.text
    assert verified.json() == {"id": profile["id"], "status": "verified", "risk_score": 20}

    # subject lookup is scoped to the caller's own records
    by_user = identity_book_client.get("/financial-identity/profiles/user/subject-book-access", headers=H)
    assert by_user.status_code == 200
    assert by_user.json()["verification_status"] == "verified"
    assert (
        identity_book_client.get("/financial-identity/profiles/user/subject-book-access", headers=H_OTHER).status_code
        == 404
    )


# --------------------------------------------------------------------------
# Forensic / IT / Operational audits (tax-audit-investigation bracket members)
# --------------------------------------------------------------------------
# Same audit-engagement engine as tax-audit, but stored under distinct node
# labels so the four audit services can never read each other's engagements
# inside the shared bracket database.


AUDIT_TRIO_MOUNTS = {
    "forensic": ("/forensic-accounting", "forensic_accounting_service", "forensic"),
    "it": ("/it-audit", "it_audit_service", "it_audit"),
    "operational": ("/operational-audit", "operational_audit_service", "operational"),
}


@pytest.fixture(scope="module")
def audit_trio_client():
    bracket = _load_bracket("tax-audit-investigation-bracket")
    for _, pkg, short in AUDIT_TRIO_MOUNTS.values():
        _patch_fake(pkg, f"{short}_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


@pytest.mark.parametrize("key", list(AUDIT_TRIO_MOUNTS))
def test_audit_trio_accessible_in_book(audit_trio_client, key):
    mount, _pkg, _short = AUDIT_TRIO_MOUNTS[key]
    payload = {
        "company_id": "co-book-access",
        "audit_type": key,
        "title": f"Book-Access {key} audit 2026",
        "objectives": ["Check controls"],
    }
    resp = audit_trio_client.post(f"{mount}/engagements", json=payload, headers=H)
    assert resp.status_code == 200, resp.text
    eng = resp.json()
    assert eng["book_id"] == BOOK
    assert eng["status"] == "planned"

    # visible in the Book, invisible to the other Book
    mine = audit_trio_client.get(f"{mount}/engagements/co-book-access", headers=H).json()
    assert any(e["id"] == eng["id"] for e in mine["engagements"])
    other = audit_trio_client.get(f"{mount}/engagements/co-book-access", headers=H_OTHER).json()
    assert all(e["id"] != eng["id"] for e in other["engagements"])

    # findings and status changes are Book-gated
    add = audit_trio_client.post(
        f"{mount}/engagements/{eng['id']}/findings",
        json={"title": "Control gap", "severity": "high", "description": "Weak control"},
        headers=H,
    )
    assert add.status_code == 200, add.text
    assert (
        audit_trio_client.post(
            f"{mount}/engagements/{eng['id']}/findings",
            json={"title": "x", "description": "y"},
            headers=H_OTHER,
        ).status_code
        == 404
    )
    assert (
        audit_trio_client.put(
            f"{mount}/engagements/{eng['id']}/status", params={"status": "completed"}, headers=H_OTHER
        ).status_code
        == 404
    )

    # complete within the Book, verify the report and cross-scope 404
    done = audit_trio_client.put(
        f"{mount}/engagements/{eng['id']}/status",
        params={"status": "completed", "summary": "Closed"},
        headers=H,
    )
    assert done.status_code == 200, done.text
    report = audit_trio_client.get(f"{mount}/report/{eng['id']}", headers=H).json()
    assert report["engagement"]["status"] == "completed"
    assert report["findings_summary"]["high"] == 1
    assert audit_trio_client.get(f"{mount}/report/{eng['id']}", headers=H_OTHER).status_code == 404

    # tax-audit's engagements never leak into this service's listings
    tax_mount = "/tax-audit"
    tax_list = audit_trio_client.get(f"{tax_mount}/engagements/co-book-access", headers=H).json()
    assert all(e["id"] != eng["id"] for e in tax_list["engagements"])


# --------------------------------------------------------------------------
# Tax compliance (tax-audit-investigation bracket member)
# --------------------------------------------------------------------------

TC_OBLIGATION_PAYLOAD = {
    "company_id": "co-book-access",
    "obligation_type": "vat_return",
    "description": "Book-Access VAT Return",
    "due_date": "2026-04-30T00:00:00Z",
    "amount": 15000,
    "filing_frequency": "quarterly",
}


@pytest.fixture(scope="module")
def tax_compliance_client():
    bracket = _load_bracket("tax-audit-investigation-bracket")
    _patch_fake("tax_compliance_service", "tc_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_tax_compliance_accessible_in_book(tax_compliance_client):
    resp = tax_compliance_client.post("/tax-compliance/obligations", json=TC_OBLIGATION_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    obl = resp.json()
    assert obl["status"] == "pending"

    # visible in the Book, invisible to the other Book and other users
    mine = tax_compliance_client.get(
        "/tax-compliance/obligations", params={"company_id": "co-book-access"}, headers=H
    ).json()
    assert any(o["id"] == obl["id"] for o in mine)
    other = tax_compliance_client.get(
        "/tax-compliance/obligations", params={"company_id": "co-book-access"}, headers=H_OTHER
    ).json()
    assert all(o["id"] != obl["id"] for o in other)

    # filing is Book-gated
    filed = tax_compliance_client.post(
        f"/tax-compliance/obligations/{obl['id']}/file",
        params={"company_id": "co-book-access", "filed_amount": 15000},
        headers=H,
    )
    assert filed.status_code == 200, filed.text
    assert filed.json()["filed"] is True

    # the other Book cannot file someone else's obligation, and cannot see it in summary
    obl2 = tax_compliance_client.post("/tax-compliance/obligations", json=TC_OBLIGATION_PAYLOAD, headers=H).json()
    blocked = tax_compliance_client.post(
        f"/tax-compliance/obligations/{obl2['id']}/file",
        params={"company_id": "co-book-access", "filed_amount": 1},
        headers=H_OTHER,
    )
    assert blocked.status_code == 404
    summary = tax_compliance_client.get(
        "/tax-compliance/summary", params={"company_id": "co-book-access"}, headers=H
    ).json()
    assert summary["filed"] == 1
    assert summary["pending"] == 1
    other_summary = tax_compliance_client.get(
        "/tax-compliance/summary", params={"company_id": "co-book-access"}, headers=H_OTHER
    ).json()
    assert other_summary["total_obligations"] == 0


# --------------------------------------------------------------------------
# Tax planning (tax-audit-investigation bracket member)
# --------------------------------------------------------------------------

TP_STRATEGY_PAYLOAD = {
    "name": "Book-Access capital allowance",
    "description": "Accelerate depreciation claims",
    "strategy_type": "deduction",
    "estimated_savings": 50000,
    "implementation_cost": 10000,
    "risk_level": "low",
    "timeframe": "short-term",
}


@pytest.fixture(scope="module")
def tax_planning_client():
    bracket = _load_bracket("tax-audit-investigation-bracket")
    _patch_fake("tax_planning_service", "tp_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_tax_planning_accessible_in_book(tax_planning_client):
    resp = tax_planning_client.post("/tax-planning/strategies", json=TP_STRATEGY_PAYLOAD, headers=H)
    assert resp.status_code == 200, resp.text
    strat = resp.json()
    assert strat["id"]

    # strategies persist and are visible in the Book, invisible to the other Book
    mine = tax_planning_client.get("/tax-planning/strategies", headers=H).json()
    assert any(s["id"] == strat["id"] for s in mine)
    other = tax_planning_client.get("/tax-planning/strategies", headers=H_OTHER).json()
    assert all(s["id"] != strat["id"] for s in other)

    # planning stays a pure computation over the request payload
    plan_payload = {
        "company_id": "co-book-access",
        "fiscal_year": 2026,
        "current_taxable_income": 1000000,
        "current_tax": 250000,
        "strategies": [TP_STRATEGY_PAYLOAD],
    }
    plan = tax_planning_client.post("/tax-planning/plan", json=plan_payload, headers=H).json()
    assert plan["projected_tax"] == 200000
    assert plan["recommended_strategies"] == ["Book-Access capital allowance"]
    # planning does not persist anything into the Book store
    assert len(tax_planning_client.get("/tax-planning/strategies", headers=H).json()) == 1


# --------------------------------------------------------------------------
# Regulatory compliance (tax-audit-investigation bracket member)
# --------------------------------------------------------------------------

RC_REGULATION_PAYLOAD = {
    "company_id": "co-book-access",
    "regulation_name": "IFRS 15 Revenue",
    "jurisdiction": "ZW",
    "framework": "IFRS",
    "requirement": "Recognize revenue when performance obligation satisfied",
    "risk_if_non_compliant": "high",
}


@pytest.fixture(scope="module")
def regulatory_compliance_client():
    bracket = _load_bracket("tax-audit-investigation-bracket")
    _patch_fake("regulatory_compliance_service", "rc_bookaccess_fake")
    with TestClient(bracket.app) as client:
        yield client


def test_regulatory_compliance_accessible_in_book(regulatory_compliance_client):
    resp = regulatory_compliance_client.post(
        "/regulatory-compliance/regulations", json=RC_REGULATION_PAYLOAD, headers=H
    )
    assert resp.status_code == 200, resp.text
    reg = resp.json()
    assert reg["status"] == "pending_review"

    # visible in the Book, invisible to the other Book and other users
    mine = regulatory_compliance_client.get(
        "/regulatory-compliance/regulations", params={"company_id": "co-book-access"}, headers=H
    ).json()
    assert any(r["id"] == reg["id"] for r in mine)
    other = regulatory_compliance_client.get(
        "/regulatory-compliance/regulations", params={"company_id": "co-book-access"}, headers=H_OTHER
    ).json()
    assert all(r["id"] != reg["id"] for r in other)

    # status updates are Book-gated
    upd = regulatory_compliance_client.post(
        f"/regulatory-compliance/regulations/{reg['id']}/update",
        params={"company_id": "co-book-access", "status": "compliant"},
        headers=H,
    )
    assert upd.status_code == 200, upd.text
    assert upd.json()["updated"] is True
    reg2 = regulatory_compliance_client.post(
        "/regulatory-compliance/regulations", json=RC_REGULATION_PAYLOAD, headers=H
    ).json()
    blocked = regulatory_compliance_client.post(
        f"/regulatory-compliance/regulations/{reg2['id']}/update",
        params={"company_id": "co-book-access", "status": "compliant"},
        headers=H_OTHER,
    )
    assert blocked.status_code == 404

    # dashboard aggregates only Book-visible regulations
    dashboard = regulatory_compliance_client.get(
        "/regulatory-compliance/dashboard", params={"company_id": "co-book-access"}, headers=H
    ).json()
    assert dashboard["total_regulations"] == 2
    assert dashboard["compliant"] == 1
    other_dashboard = regulatory_compliance_client.get(
        "/regulatory-compliance/dashboard", params={"company_id": "co-book-access"}, headers=H_OTHER
    ).json()
    assert other_dashboard["total_regulations"] == 0
