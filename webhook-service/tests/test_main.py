"""Book-scoping and persistence tests for webhook-service (fake Neo4j harness)."""

import importlib.util
import os

import main
import pytest
from fastapi.testclient import TestClient
from webhook_service.database import Neo4jConnector

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("wh_fake", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)
FakeSession = _fake_mod.FakeSession

_fake_session = FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))

client = TestClient(app)


class _FakeAsyncClient:
    """Replaces httpx.AsyncClient: no real network access."""

    def __init__(self, timeout=10):
        pass

    async def __aenter__(self):
        return self

    async def __aexit__(self, *exc):
        return False

    async def post(self, url, content=None, headers=None):
        raise RuntimeError("network disabled in tests")


@pytest.fixture(autouse=True)
def _clean_fake_graph(monkeypatch):
    monkeypatch.setattr(main.httpx, "AsyncClient", _FakeAsyncClient)
    _fake_session.nodes.clear()
    _fake_session.edges.clear()
    yield
    _fake_session.nodes.clear()
    _fake_session.edges.clear()


U1, U2 = "wh-user-1", "wh-user-2"
BOOK_A, BOOK_B = "wh-book-a", "wh-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H1_PERSONAL = {"X-User-Id": U1}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _ep_payload(company="co-wh", url="https://example.com/hook", **kw):
    payload = {"company_id": company, "url": url}
    payload.update(kw)
    return payload


def test_create_list_persist():
    resp = client.post("/endpoints", json=_ep_payload(events=["invoice.created"]), headers=H1)
    assert resp.status_code == 200, resp.text
    ep = resp.json()
    assert ep["events"] == ["invoice.created"]

    listed = client.get("/endpoints/co-wh", headers=H1).json()
    assert listed["total"] == 1
    # persists across requests
    assert client.get("/endpoints/co-wh", headers=H1).json()["total"] == 1
    # other user sees nothing
    assert client.get("/endpoints/co-wh", headers=H2).json()["total"] == 0


def test_dispatch_scopes_and_records():
    ep = client.post("/endpoints", json=_ep_payload(company="co-disp", events=["invoice.created"]), headers=H1).json()
    # endpoint subscribed to another event is skipped
    client.post("/endpoints", json=_ep_payload(company="co-disp", events=["other.event"]), headers=H1)

    resp = client.post("/dispatch/co-disp", params={"event_type": "invoice.created"}, json={"amount": 10}, headers=H1)
    assert resp.status_code == 200, resp.text
    data = resp.json()
    assert data["total_sent"] == 1  # only the matching endpoint
    assert data["successful"] == 0  # network disabled -> failed
    assert data["deliveries"][0]["status"] == "failed"

    # delivery persisted and scoped to owner + Book
    stored = client.get(f"/deliveries/{ep['id']}", headers=H1).json()
    assert stored["total"] == 1
    assert stored["deliveries"][0]["event_type"] == "invoice.created"
    assert client.get(f"/deliveries/{ep['id']}", headers=H2).json()["total"] == 0

    # other user's dispatch never reaches U1's endpoints
    resp2 = client.post("/dispatch/co-disp", params={"event_type": "invoice.created"}, json={"x": 1}, headers=H2)
    assert resp2.json()["total_sent"] == 0


def test_dispatch_wildcard_endpoint():
    # empty events list means subscribed to everything
    ep = client.post("/endpoints", json=_ep_payload(company="co-wild"), headers=H1).json()
    data = client.post("/dispatch/co-wild", params={"event_type": "anything.at_all"}, json={}, headers=H1).json()
    assert data["total_sent"] == 1
    assert client.get(f"/deliveries/{ep['id']}", headers=H1).json()["total"] == 1


def test_book_a_b_isolation():
    ep_a = client.post("/endpoints", json=_ep_payload(company="co-a", url="https://a.example/hook"), headers=H1).json()
    client.post(
        "/endpoints",
        json=_ep_payload(company="co-a", url="https://b.example/hook"),
        headers={"X-User-Id": U1, "X-Book-ID": BOOK_B},
    )
    assert client.get("/endpoints/co-a", headers=H1).json()["total"] == 1
    other_book = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    assert client.get("/endpoints/co-a", headers=other_book).json()["total"] == 1
    # personal spans books
    assert client.get("/endpoints/co-a", headers=H1_PERSONAL).json()["total"] == 2
    # dispatch is Book-gated: Book A dispatch only hits the Book A endpoint
    data = client.post("/dispatch/co-a", params={"event_type": "e"}, json={}, headers=H1).json()
    assert data["total_sent"] == 1
    stored = client.get(f"/deliveries/{ep_a['id']}", headers=H1).json()
    assert stored["total"] == 1


def test_x_user_id_required():
    assert client.post("/endpoints", json=_ep_payload()).status_code in (401, 403, 422)
    assert client.get("/endpoints/co-wh").status_code in (401, 403, 422)
    assert client.post("/dispatch/co-wh", params={"event_type": "e"}, json={}).status_code in (401, 403, 422)
