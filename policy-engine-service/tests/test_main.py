"""Book-scoping and persistence tests for policy-engine-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from policy_engine_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("pe_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "pe-user-1", "pe-user-2"
BOOK_A, BOOK_B = "pe-book-a", "pe-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _rule(name="Max Amount", resource="transaction", field="amount", op=">", value=50000, **kw):
    payload = {
        "name": name,
        "resource_type": resource,
        "condition_field": field,
        "condition_operator": op,
        "condition_value": value,
    }
    payload.update(kw)
    return payload


def test_create_list_persist():
    resp = client.post("/rules/co-pe", json=_rule(action="require_approval", message="Too big"), headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["action"] == "require_approval"

    listed = client.get("/rules/co-pe", headers=H1).json()
    assert listed["total"] == 1
    rule = listed["rules"][0]
    assert rule["condition_value"] == 50000
    assert rule["book_id"] == BOOK_A
    # persists across requests
    assert client.get("/rules/co-pe", headers=H1).json()["total"] == 1
    # other user sees nothing
    assert client.get("/rules/co-pe", headers=H2).json()["total"] == 0


def test_evaluate_semantics_and_scoping():
    client.post("/rules/co-ev", json=_rule(action="deny", message="blocked"), headers=H1)
    client.post(
        "/rules/co-ev",
        json=_rule(name="Contains check", field="category", op="contains", value="alcohol", action="deny"),
        headers=H1,
    )

    resp = client.post("/evaluate/co-ev", params={"resource_type": "transaction"}, json={"amount": 80000}, headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["triggered_count"] == 1
    assert data["blocked"] is True
    assert data["allowed"] is False
    assert data["evaluations"][0]["rule_name"] == "Max Amount"

    # other users' rules never apply to this caller
    other = client.post(
        "/evaluate/co-ev", params={"resource_type": "transaction"}, json={"amount": 80000}, headers=H2
    ).json()
    assert other["triggered_count"] == 0
    assert other["allowed"] is True

    # non-matching resource_type ignored
    data = client.post(
        "/evaluate/co-ev", params={"resource_type": "invoice"}, json={"amount": 80000}, headers=H1
    ).json()
    assert data["triggered_count"] == 0

    # operators: <, ==, >=, <=, contains
    client.post("/rules/co-op", json=_rule(name="Small", op="<", value=100), headers=H1)
    client.post("/rules/co-op", json=_rule(name="Eq", op="==", value=42), headers=H1)
    client.post("/rules/co-op", json=_rule(name="Ge", op=">=", value=10), headers=H1)
    client.post("/rules/co-op", json=_rule(name="Le", op="<=", value=999), headers=H1)
    client.post("/rules/co-op", json=_rule(name="C", field="memo", op="contains", value="urgent"), headers=H1)
    data = client.post(
        "/evaluate/co-op",
        params={"resource_type": "transaction"},
        json={"amount": 50, "memo": "very urgent memo"},
        headers=H1,
    ).json()
    assert data["triggered_count"] == 4  # Small, Eq? no 50!=42... Ge, Le, C
    # exact: Small(50<100) + Ge(50>=10) + Le(50<=999) + C(contains) = 4


def test_book_a_b_isolation():
    client.post("/rules/co-a", json=_rule(name="A Rule"), headers=H1)
    client.post("/rules/co-a", json=_rule(name="B Rule"), headers={"X-User-Id": U1, "X-Book-ID": BOOK_B})
    names_a = [r["name"] for r in client.get("/rules/co-a", headers=H1).json()["rules"]]
    assert names_a == ["A Rule"]
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    names_b = [r["name"] for r in client.get("/rules/co-a", headers=other_book).json()["rules"]]
    assert names_b == ["B Rule"]
    # personal spans books
    names_p = [r["name"] for r in client.get("/rules/co-a", headers=H1_PERSONAL).json()["rules"]]
    assert set(names_p) == {"A Rule", "B Rule"}
    # evaluation only sees the Book's rules
    data = client.post(
        "/evaluate/co-a", params={"resource_type": "transaction"}, json={"amount": 999999}, headers=other_book
    ).json()
    assert data["triggered_count"] == 1


def test_x_user_id_required():
    assert client.post("/rules/co-pe", json=_rule()).status_code in (401, 403, 422)
    assert client.get("/rules/co-pe").status_code in (401, 403, 422)
    assert client.post("/evaluate/co-pe", params={"resource_type": "transaction"}, json={}).status_code in (
        401,
        403,
        422,
    )
