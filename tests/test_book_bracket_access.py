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
