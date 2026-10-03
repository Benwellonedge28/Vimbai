"""Book-scoping and persistence tests for intercompany-service (fake Neo4j harness)."""

import importlib.util
import os

import main  # noqa: F401 (bootstraps the intercompany_service package alias)
import pytest
from fastapi.testclient import TestClient
from intercompany_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ic_root_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "ic-user-1", "ic-user-2"
BOOK_A, BOOK_B = "ic-book-a", "ic-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _make_entities(headers=H1):
    e1 = client.post(
        "/entities", params={"name": "Vimbai ZW", "legal_entity_code": "VZ001", "currency": "USD"}, headers=headers
    ).json()
    e2 = client.post(
        "/entities", params={"name": "Vimbai ZA", "legal_entity_code": "VZ002", "currency": "USD"}, headers=headers
    ).json()
    return e1, e2


def _make_txn(from_id, to_id, amount=10000.0, ttype="service_fee", headers=H1):
    return client.post(
        "/transactions",
        params={
            "from_entity_id": from_id,
            "to_entity_id": to_id,
            "transaction_type": ttype,
            "amount": amount,
        },
        headers=headers,
    ).json()


def test_health():
    assert client.get("/health").json()["service"] == "intercompany-service"


def test_entity_scoping():
    e1, _ = _make_entities()
    assert e1["legal_entity_code"] == "VZ001"

    # foreign caller sees nothing; Book-gated listing too
    assert client.get("/entities", headers=H2).json() == []
    other_book = dict(H1)
    other_book["X-Book-ID"] = BOOK_B
    assert client.get("/entities", headers=other_book).json() == []
    assert len(client.get("/entities", headers=H1).json()) == 2


def test_invalid_transaction_type():
    e1, e2 = _make_entities()
    r = client.post(
        "/transactions",
        params={
            "from_entity_id": e1["id"],
            "to_entity_id": e2["id"],
            "transaction_type": "bribe",
            "amount": 10.0,
        },
        headers=H1,
    )
    assert r.status_code == 400


def test_match_and_elimination_scoped():
    e1, e2 = _make_entities()
    t1 = _make_txn(e1["id"], e2["id"], 10000.0)
    t2 = _make_txn(e2["id"], e1["id"], 10000.0, ttype="cost_allocation")

    # amount mismatch: 400
    t3 = _make_txn(e1["id"], e2["id"], 5000.0, ttype="loan")
    r = client.post("/transactions/match", params={"txn1_id": t1["id"], "txn2_id": t3["id"]}, headers=H1)
    assert r.status_code == 400

    # foreign caller cannot match or eliminate: 404
    assert (
        client.post("/transactions/match", params={"txn1_id": t1["id"], "txn2_id": t2["id"]}, headers=H2).status_code
        == 404
    )

    # owner matches the mirrored pair
    r = client.post("/transactions/match", params={"txn1_id": t1["id"], "txn2_id": t2["id"]}, headers=H1)
    assert r.status_code == 200
    body = r.json()
    assert body["matched"] is True

    # matched status persisted on both sides, with cross-references
    txns = client.get("/transactions", headers=H1).json()
    by_id = {t["id"]: t for t in txns}
    assert by_id[t1["id"]]["status"] == "matched"
    assert by_id[t1["id"]]["matched_transaction_id"] == t2["id"]
    assert by_id[t2["id"]]["status"] == "matched"
    assert by_id[t2["id"]]["matched_transaction_id"] == t1["id"]

    # elimination entry persisted, scoped to the owner
    elims = client.get("/eliminations", headers=H1).json()
    assert len(elims) == 1
    assert elims[0]["id"] == body["elimination_id"]
    assert elims[0]["pair_id"] == f"{t1['id']}:{t2['id']}"
    assert elims[0]["amount"] == 10000.0
    assert elims[0]["debit_entity_id"] == e1["id"]
    assert elims[0]["credit_entity_id"] == e2["id"]
    assert client.get("/eliminations", headers=H2).json() == []
    other_book = dict(H1)
    other_book["X-Book-ID"] = BOOK_B
    assert client.get("/eliminations", headers=other_book).json() == []


def test_transaction_filters():
    e1, e2 = _make_entities()
    t1 = _make_txn(e1["id"], e2["id"], 1000.0, ttype="loan")
    _make_txn(e2["id"], e1["id"], 2000.0, ttype="royalty")

    assert len(client.get("/transactions", params={"from_entity": e1["id"]}, headers=H1).json()) == 1
    assert len(client.get("/transactions", params={"to_entity": e1["id"]}, headers=H1).json()) == 1
    assert len(client.get("/transactions", params={"status": "pending"}, headers=H1).json()) == 2
    # foreign caller: nothing visible under any filter
    assert client.get("/transactions", params={"status": "pending"}, headers=H2).json() == []
    assert t1["status"] == "pending"
