"""Book-scoping and persistence tests for financial-integrity-service (fake Neo4j harness).

Covers: check semantics (balance/hash/completeness), report
aggregation, ownership and Book isolation.
"""

import hashlib
import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from financial_integrity_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("fi_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "fi-user-1", "fi-user-2"
BOOK_A, BOOK_B = "fi-book-a", "fi-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def test_balance_check_semantics():
    r = client.post(
        "/check/balance",
        params={"company_id": "co-fi", "account_id": "acc-1", "debits": 10000, "credits": 10000},
        headers=H1,
    )
    assert r.json() == {"passed": True, "difference": 0.0, "tolerance": 0.01}
    r = client.post(
        "/check/balance",
        params={"company_id": "co-fi", "account_id": "acc-1", "debits": 10000, "credits": 9000},
        headers=H1,
    )
    assert r.json()["passed"] is False
    assert r.json()["difference"] == 1000.0
    # custom tolerance
    r = client.post(
        "/check/balance",
        params={"company_id": "co-fi", "account_id": "acc-1", "debits": 100, "credits": 100.5, "tolerance": 1.0},
        headers=H1,
    )
    assert r.json()["passed"] is True


def test_hash_check():
    data = "ledger-entry-42"
    good = hashlib.sha256(data.encode()).hexdigest()
    r = client.post(
        "/check/hash",
        params={
            "company_id": "co-fi",
            "entity_type": "transaction",
            "entity_id": "tx-42",
            "data": data,
            "expected_hash": good,
        },
        headers=H1,
    )
    body = r.json()
    assert body == {"passed": True, "actual_hash": good, "expected_hash": good}
    r = client.post(
        "/check/hash",
        params={
            "company_id": "co-fi",
            "entity_type": "transaction",
            "entity_id": "tx-42",
            "data": data,
            "expected_hash": "deadbeef",
        },
        headers=H1,
    )
    assert r.json()["passed"] is False
    assert r.json()["actual_hash"] == good


def test_completeness_check():
    r = client.post(
        "/check/completeness",
        params={"company_id": "co-fi", "entity_type": "transactions", "expected_count": 100, "actual_count": 98},
        headers=H1,
    )
    body = r.json()
    assert body == {"passed": False, "expected": 100, "actual": 98, "missing": 2}
    r = client.post(
        "/check/completeness",
        params={"company_id": "co-fi", "entity_type": "transactions", "expected_count": 10, "actual_count": 10},
        headers=H1,
    )
    assert r.json()["passed"] is True


def test_report_aggregates_only_own_checks():
    # balance pass, balance fail, hash pass, completeness fail
    client.post(
        "/check/balance", params={"company_id": "co-fi", "account_id": "a", "debits": 5, "credits": 5}, headers=H1
    )
    client.post(
        "/check/balance", params={"company_id": "co-fi", "account_id": "b", "debits": 5, "credits": 4}, headers=H1
    )
    client.post(
        "/check/hash",
        params={
            "company_id": "co-fi",
            "entity_type": "t",
            "entity_id": "e",
            "data": "x",
            "expected_hash": hashlib.sha256(b"x").hexdigest(),
        },
        headers=H1,
    )
    client.post(
        "/check/completeness",
        params={"company_id": "co-fi", "entity_type": "t", "expected_count": 3, "actual_count": 1},
        headers=H1,
    )

    report = client.get("/report/co-fi", headers=H1).json()
    assert report["total_checks"] == 4
    assert report["passed"] == 2
    assert report["failed"] == 2
    assert report["pass_rate"] == 50.0
    types = [c["check_type"] for c in report["checks"]]
    assert types == ["balance_check", "balance_check", "hash_verify", "completeness"]

    # a different company's report is empty, another user's report is empty
    assert client.get("/report/co-other", headers=H1).json()["total_checks"] == 0
    assert client.get("/report/co-fi", headers=H2).json()["total_checks"] == 0


def test_book_a_b_isolation():
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    client.post(
        "/check/balance", params={"company_id": "co-fi", "account_id": "a", "debits": 5, "credits": 5}, headers=H1
    )
    client.post(
        "/check/balance", params={"company_id": "co-fi", "account_id": "a", "debits": 5, "credits": 4}, headers=hb
    )
    ra = client.get("/report/co-fi", headers=H1).json()
    rb = client.get("/report/co-fi", headers=hb).json()
    assert ra["total_checks"] == 1 and ra["passed"] == 1
    assert rb["total_checks"] == 1 and rb["passed"] == 0
    assert ra["pass_rate"] == 100.0
    assert rb["pass_rate"] == 0.0


def test_x_user_id_required():
    assert client.post(
        "/check/balance", params={"company_id": "co", "account_id": "a", "debits": 1, "credits": 1}
    ).status_code in (401, 403, 422)
    assert client.get("/report/co").status_code in (401, 403, 422)
