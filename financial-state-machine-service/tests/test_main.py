"""Book-scoping and persistence tests for financial-state-machine-service (fake Neo4j harness).

Covers: document CRUD persistence, transition validation, history,
ownership and Book isolation.
"""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from financial_state_machine_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("fsm_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "fsm-user-1", "fsm-user-2"
BOOK_A, BOOK_B = "fsm-book-a", "fsm-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _create(doc_type="invoice", company="co-fsm", reference="INV-001", headers=H1):
    r = client.post(
        "/documents", json={"company_id": company, "document_type": doc_type, "reference": reference}, headers=headers
    )
    assert r.status_code == 200, r.text
    return r.json()["id"]


def test_create_and_full_lifecycle():
    doc_id = _create()
    doc = client.get(f"/documents/{doc_id}", headers=H1).json()
    assert doc["current_state"] == "draft"
    assert doc["document_type"] == "invoice"
    assert doc["history"] == []

    # draft -> pending_approval -> approved -> posted -> archived
    for state in ("pending_approval", "approved", "posted", "archived"):
        r = client.post(
            f"/documents/{doc_id}/transition", params={"to_state": state, "user_id": "approver-1"}, headers=H1
        )
        assert r.status_code == 200, r.text
        assert r.json()["current_state"] == state
        assert r.json()["history_count"] == ("pending_approval", "approved", "posted", "archived").index(state) + 1

    # terminal state: no further transitions
    r = client.post(f"/documents/{doc_id}/transition", params={"to_state": "draft"}, headers=H1)
    assert r.status_code == 400

    history = client.get(f"/documents/{doc_id}/history", headers=H1).json()
    assert history["current_state"] == "archived"
    assert len(history["history"]) == 4
    assert history["history"][0]["from_state"] == "draft"
    assert history["history"][0]["to_state"] == "pending_approval"
    assert history["history"][0]["user_id"] == "approver-1"


def test_invalid_transitions_400():
    doc_id = _create(company="co-2")
    # draft -> posted is not allowed
    r = client.post(f"/documents/{doc_id}/transition", params={"to_state": "posted"}, headers=H1)
    assert r.status_code == 400
    assert "Invalid transition" in r.json()["detail"]
    assert client.get(f"/documents/{doc_id}", headers=H1).json()["current_state"] == "draft"
    # draft -> cancelled is allowed and terminal
    assert (
        client.post(f"/documents/{doc_id}/transition", params={"to_state": "cancelled"}, headers=H1).status_code == 200
    )
    r = client.post(f"/documents/{doc_id}/transition", params={"to_state": "approved"}, headers=H1)
    assert r.status_code == 400


def test_scoping_404():
    doc_id = _create()
    # cross-user and cross-Book access 404 (read, transition, history)
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get(f"/documents/{doc_id}", headers=H2).status_code == 404
    assert client.get(f"/documents/{doc_id}", headers=hb).status_code == 404
    assert (
        client.post(f"/documents/{doc_id}/transition", params={"to_state": "pending_approval"}, headers=H2).status_code
        == 404
    )
    assert client.get(f"/documents/{doc_id}/history", headers=H2).status_code == 404
    # original doc untouched
    assert client.get(f"/documents/{doc_id}", headers=H1).json()["current_state"] == "draft"


def test_book_a_b_isolation():
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    da = _create(reference="A-1", headers=H1)
    db = _create(reference="B-1", headers=hb)
    client.post(f"/documents/{da}/transition", params={"to_state": "pending_approval"}, headers=H1)
    # Book A doc still draft in Book B view? different doc ids; each Book sees only its own
    assert client.get(f"/documents/{da}", headers=H1).json()["current_state"] == "pending_approval"
    assert client.get(f"/documents/{db}", headers=hb).json()["current_state"] == "draft"
    assert client.get(f"/documents/{db}", headers=H1).status_code == 404


def test_states_catalogue():
    r = client.get("/states")
    assert r.status_code == 200
    data = r.json()
    assert data["states"] == ["draft", "pending_approval", "approved", "posted", "cancelled", "archived"]
    assert data["transitions"]["draft"] == ["pending_approval", "cancelled"]
    assert data["transitions"]["posted"] == ["archived"]
    assert data["transitions"]["cancelled"] == []


def test_x_user_id_required():
    assert client.post("/documents", json={"company_id": "co"}).status_code in (401, 403, 422)
    assert client.get("/documents/whatever").status_code in (401, 403, 422)
