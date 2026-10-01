"""Book-scoping and persistence tests for disaster-recovery-service (fake Neo4j harness).

Covers: item CRUD persistence, soft-delete semantics, metrics
scoping, ownership and Book isolation.
"""

import importlib.util
import os

import main
import pytest
from disaster_recovery_service.database import Neo4jConnector
from fastapi.testclient import TestClient

app = main.app

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("dr_fake", os.path.join(_HERE, "fake_neo4j.py"))
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


U1, U2 = "dr-user-1", "dr-user-2"
BOOK_A, BOOK_B = "dr-book-a", "dr-book-b"
H1 = {"X-User-Id": U1, "X-Book-ID": BOOK_A}
H2 = {"X-User-Id": U2, "X-Book-ID": BOOK_A}


def _item(name="Failover plan", **kw):
    p = {
        "name": name,
        "description": "Primary datacenter failover",
        "config": {"rpo": 15, "rto": 60},
        "status": "active",
    }
    p.update(kw)
    return p


def test_create_list_persist():
    resp = client.post("/items", params={"company_id": "co-dr"}, json=_item(), headers=H1)
    assert resp.status_code == 200, resp.text
    created = resp.json()
    assert created["status"] == "created"

    listed = client.get("/items/co-dr", headers=H1).json()
    assert listed["total"] == 1
    item = listed["items"][0]
    assert item["name"] == "Failover plan"
    assert item["config"] == {"rpo": 15, "rto": 60}
    assert item["id"] == created["id"]

    # other user sees nothing
    assert client.get("/items/co-dr", headers=H2).json()["total"] == 0


def test_update_semantics_and_scoping():
    created = client.post("/items", params={"company_id": "co-dr"}, json=_item(), headers=H1).json()
    iid = created["id"]

    r = client.put(f"/items/{iid}", params={"name": "Failover v2", "status": "tested"}, headers=H1)
    assert r.status_code == 200, r.text
    assert r.json() == {"id": iid, "status": "updated"}
    after = client.get("/items/co-dr", headers=H1).json()["items"][0]
    assert after["name"] == "Failover v2"
    assert after["status"] == "tested"
    assert after["description"] == "Primary datacenter failover"  # untouched

    # cross-user / cross-Book update 404
    assert client.put(f"/items/{iid}", params={"name": "hijack"}, headers=H2).status_code == 404
    assert (
        client.put(
            f"/items/{iid}", params={"name": "hijack"}, headers={"X-User-Id": U1, "X-Book-ID": BOOK_B}
        ).status_code
        == 404
    )
    assert client.get("/items/co-dr", headers=H1).json()["items"][0]["name"] == "Failover v2"


def test_soft_delete_semantics():
    created = client.post("/items", params={"company_id": "co-dr"}, json=_item(), headers=H1).json()
    iid = created["id"]
    assert client.delete(f"/items/{iid}", headers=H1).json() == {"id": iid, "status": "deleted"}
    # soft delete: still listed with status flipped (original mock semantics)
    listed = client.get("/items/co-dr", headers=H1).json()
    assert listed["total"] == 1
    assert listed["items"][0]["status"] == "deleted"
    # cross-scope delete 404, and a second delete of the same item still 200 (status re-set)
    assert client.delete(f"/items/{iid}", headers=H2).status_code == 404
    assert client.delete(f"/items/{iid}", headers=H1).status_code == 200


def test_book_a_b_isolation():
    hb = {"X-User-Id": U1, "X-Book-ID": BOOK_B}
    client.post("/items", params={"company_id": "co-dr"}, json=_item(name="Book A plan"), headers=H1)
    client.post("/items", params={"company_id": "co-dr"}, json=_item(name="Book B plan"), headers=hb)
    a = client.get("/items/co-dr", headers=H1).json()["items"]
    b = client.get("/items/co-dr", headers=hb).json()["items"]
    assert [i["name"] for i in a] == ["Book A plan"]
    assert [i["name"] for i in b] == ["Book B plan"]
    # metrics are Book-scoped
    m1 = client.get("/metrics", headers=H1).json()
    assert m1["total_items"] == 1
    assert m1["companies"] == 1
    assert m1["service"] == "disaster-recovery-service"


def test_metrics_across_companies():
    client.post("/items", params={"company_id": "co-1"}, json=_item(), headers=H1)
    client.post("/items", params={"company_id": "co-2"}, json=_item(name="Backup plan"), headers=H1)
    m = client.get("/metrics", headers=H1).json()
    assert m["total_items"] == 2
    assert m["companies"] == 2
    # other user: zero
    assert client.get("/metrics", headers=H2).json()["total_items"] == 0


def test_x_user_id_required():
    assert client.post("/items", params={"company_id": "co-dr"}, json=_item()).status_code in (401, 403, 422)
    assert client.get("/items/co-dr").status_code in (401, 403, 422)
    assert client.get("/metrics").status_code in (401, 403, 422)
