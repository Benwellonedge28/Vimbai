"""
Vimbai Workflow Service - Test Suite
Tests: definition CRUD, instance CRUD, Book isolation (X-Book-ID scoping).
"""

import os
from datetime import datetime, timedelta, timezone

import pytest
from fastapi.testclient import TestClient

os.environ.setdefault("JWT_SECRET", "test-secret-key-for-testing-only")

import workflow_service.dependencies as deps
from workflow_service.dependencies import get_db_session
from main import app

client = TestClient(app)

BOOK_A = {"X-Book-ID": "book-aaa-111"}
BOOK_B = {"X-Book-ID": "book-bbb-222"}


# ---------------------------------------------------------------------------
# Fake Neo4j async session: interprets the exact Cypher patterns crud uses.
# ---------------------------------------------------------------------------
class Counters:
    def __init__(self, nodes_deleted=0):
        self.nodes_deleted = nodes_deleted


class Summary:
    def __init__(self, counters):
        self.counters = counters


class FakeResult:
    def __init__(self, records=None, nodes_deleted=0):
        self._records = records or []
        self._summary = Summary(Counters(nodes_deleted))

    async def single(self):
        return self._records[0] if self._records else None

    def __aiter__(self):
        self._iter = iter(self._records)
        return self

    async def __anext__(self):
        try:
            return next(self._iter)
        except StopIteration:
            raise StopAsyncIteration

    def consume(self):
        return self._summary


class FakeSession:
    def __init__(self):
        self.definitions = []
        self.instances = []

    @staticmethod
    def _visible(node, params):
        return params.get("book_id") is None or node.get("book_id") == params.get("book_id")

    async def run(self, query, params=None, **kw):
        merged = dict(params or {})
        merged.update(kw)

        if "CREATE (wd:WorkflowDefinition $props)" in query:
            node = dict(merged["props"])
            self.definitions.append(node)
            return FakeResult([{"wd": node}])

        if "CREATE (wi:WorkflowInstance $props)" in query:
            parent = [
                d for d in self.definitions if d["id"] == merged["workflow_definition_id"] and self._visible(d, merged)
            ]
            if not parent:
                return FakeResult()  # MATCH guard found nothing
            node = dict(merged["props"])
            self.instances.append(node)
            return FakeResult([{"wi": node}])

        if "WorkflowDefinition" in query and "DETACH DELETE wd" in query:
            found = [d for d in self.definitions if d["id"] == merged["definition_id"] and self._visible(d, merged)]
            for d in found:
                self.definitions.remove(d)
            return FakeResult(nodes_deleted=len(found))

        if "WorkflowInstance" in query and "DETACH DELETE wi" in query:
            found = [i for i in self.instances if i["id"] == merged["instance_id"] and self._visible(i, merged)]
            for i in found:
                self.instances.remove(i)
            return FakeResult(nodes_deleted=len(found))

        if "WorkflowDefinition" in query and "SET" in query:  # update definition
            found = [d for d in self.definitions if d["id"] == merged["definition_id"] and self._visible(d, merged)]
            if not found:
                return FakeResult()
            d = found[0]
            for k, v in merged.items():
                if k not in ("book_id", "definition_id"):
                    d[k] = v
            return FakeResult([{"wd": d}])

        if "WorkflowInstance" in query and "SET" in query:  # update instance
            found = [i for i in self.instances if i["id"] == merged["instance_id"] and self._visible(i, merged)]
            if not found:
                return FakeResult()
            i = found[0]
            for k, v in merged.items():
                if k not in ("book_id", "instance_id"):
                    i[k] = v
            return FakeResult([{"wi": i}])

        if "trigger_event: $trigger_event" in query:
            found = [
                d
                for d in self.definitions
                if d.get("trigger_event") == merged["trigger_event"] and d.get("is_active") and self._visible(d, merged)
            ]
            return FakeResult([{"wd": d} for d in found])

        if "MATCH (wd:WorkflowDefinition)" in query and "ORDER BY wd.name" in query:
            found = [d for d in self.definitions if self._visible(d, merged)]
            found.sort(key=lambda d: d.get("name", ""))
            return FakeResult([{"wd": d} for d in found])

        if "MATCH (wd:WorkflowDefinition {id: $definition_id})" in query:
            found = [d for d in self.definitions if d["id"] == merged["definition_id"] and self._visible(d, merged)]
            return FakeResult([{"wd": found[0]}] if found else [])

        if "MATCH (wi:WorkflowInstance)-[:BASED_ON]->" in query:
            found = [
                i
                for i in self.instances
                if i["workflow_definition_id"] == merged["definition_id"] and self._visible(i, merged)
            ]
            found.sort(key=lambda i: i.get("start_date", ""), reverse=True)
            return FakeResult([{"wi": i} for i in found])

        if "MATCH (wi:WorkflowInstance {id: $instance_id})" in query:
            found = [i for i in self.instances if i["id"] == merged["instance_id"] and self._visible(i, merged)]
            return FakeResult([{"wi": found[0]}] if found else [])

        raise AssertionError(f"FakeSession: unhandled query: {query[:80]!r}")


@pytest.fixture
def fake_session():
    return FakeSession()


@pytest.fixture(autouse=True)
def override_db(fake_session):
    async def _get():
        yield fake_session

    app.dependency_overrides[get_db_session] = _get
    yield
    app.dependency_overrides.pop(get_db_session, None)


@pytest.fixture
def auth_headers():
    import jwt as pyjwt

    token = pyjwt.encode(
        {
            "user_id": "test-user-id",
            "username": "testuser",
            "role": "admin",
            "permissions": ["workflow.*"],
            "exp": datetime.now(timezone.utc) + timedelta(hours=1),
        },
        os.environ["JWT_SECRET"],
        algorithm="HS256",
    )
    return {"Authorization": f"Bearer {token}"}


def _definition_payload(name="Invoice Approval Workflow"):
    return {
        "name": name,
        "trigger_event": "InvoiceCreated",
        "description": "Route invoices through approvals",
        "is_active": True,
        "steps": [
            {
                "step_id": "s1",
                "name": "Manager review",
                "step_type": "approval",
                "assignee_role": "FinanceManager",
                "config": {},
                "next_steps": [],
            }
        ],
    }


def _create_definition(headers, **overrides):
    r = client.post("/workflow-definitions/", json={**_definition_payload(), **overrides}, headers=headers)
    assert r.status_code == 201, r.text
    return r.json()


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200


class TestAuth:
    def test_create_definition_no_auth(self):
        response = client.post("/workflow-definitions/", json=_definition_payload())
        assert response.status_code == 401

    def test_get_definitions_no_auth(self):
        response = client.get("/workflow-definitions/")
        assert response.status_code == 401

    def test_missing_permission_rejected(self):
        import jwt as pyjwt

        token = pyjwt.encode(
            {
                "user_id": "u1",
                "username": "limited",
                "role": "viewer",
                "permissions": ["reporting.read"],
                "exp": datetime.now(timezone.utc) + timedelta(hours=1),
            },
            os.environ["JWT_SECRET"],
            algorithm="HS256",
        )
        response = client.post(
            "/workflow-definitions/", json=_definition_payload(), headers={"Authorization": f"Bearer {token}"}
        )
        assert response.status_code == 403


class TestDefinitionCRUD:
    def test_create_and_get_definition(self, auth_headers):
        created = _create_definition(auth_headers)
        r = client.get(f"/workflow-definitions/{created['id']}", headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["name"] == "Invoice Approval Workflow"
        assert r.json()["steps"][0]["step_id"] == "s1"

    def test_list_definitions(self, auth_headers):
        _create_definition(auth_headers, name="Workflow One")
        _create_definition(auth_headers, name="Workflow Two")
        r = client.get("/workflow-definitions/", headers=auth_headers)
        assert r.status_code == 200
        assert len(r.json()) == 2

    def test_update_definition(self, auth_headers):
        created = _create_definition(auth_headers)
        r = client.put(
            f"/workflow-definitions/{created['id']}", json={"name": "Renamed Workflow"}, headers=auth_headers
        )
        assert r.status_code == 200
        assert r.json()["name"] == "Renamed Workflow"

    def test_delete_definition(self, auth_headers):
        created = _create_definition(auth_headers)
        r = client.delete(f"/workflow-definitions/{created['id']}", headers=auth_headers)
        assert r.status_code == 204

    def test_unknown_definition_404(self, auth_headers):
        r = client.get("/workflow-definitions/nope-404", headers=auth_headers)
        assert r.status_code == 404

    def test_missing_fields_rejected(self, auth_headers):
        r = client.post("/workflow-definitions/", json={"name": "x"}, headers=auth_headers)
        assert r.status_code == 422


class TestInstanceCRUD:
    def test_create_and_get_instance(self, auth_headers):
        definition = _create_definition(auth_headers)
        r = client.post(
            "/workflow-instances/",
            json={
                "workflow_definition_id": definition["id"],
                "triggered_by_event": "InvoiceCreated",
                "context": {"invoice_id": "inv-1"},
            },
            headers=auth_headers,
        )
        assert r.status_code == 201, r.text
        instance_id = r.json()["id"]

        r = client.get(f"/workflow-instances/{instance_id}", headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["status"] == "running"

    def test_instance_unknown_definition_404(self, auth_headers):
        r = client.post(
            "/workflow-instances/",
            json={"workflow_definition_id": "nope", "triggered_by_event": "x"},
            headers=auth_headers,
        )
        assert r.status_code == 404

    def test_update_instance(self, auth_headers):
        definition = _create_definition(auth_headers)
        r = client.post(
            "/workflow-instances/",
            json={"workflow_definition_id": definition["id"], "triggered_by_event": "ev"},
            headers=auth_headers,
        )
        instance_id = r.json()["id"]
        r = client.put(f"/workflow-instances/{instance_id}", json={"status": "completed"}, headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["status"] == "completed"

    def test_delete_instance(self, auth_headers):
        definition = _create_definition(auth_headers)
        r = client.post(
            "/workflow-instances/",
            json={"workflow_definition_id": definition["id"], "triggered_by_event": "ev"},
            headers=auth_headers,
        )
        instance_id = r.json()["id"]
        r = client.delete(f"/workflow-instances/{instance_id}", headers=auth_headers)
        assert r.status_code == 204

    def test_unknown_instance_404(self, auth_headers):
        r = client.get("/workflow-instances/nope-404", headers=auth_headers)
        assert r.status_code == 404


class TestBookIsolation:
    def test_definition_stamped_with_book_id(self, auth_headers, fake_session):
        _create_definition({**auth_headers, **BOOK_A})
        assert fake_session.definitions[0]["book_id"] == "book-aaa-111"

        # unscoped (personal) definitions keep book_id = None
        _create_definition(auth_headers, name="Personal Workflow")
        assert fake_session.definitions[1]["book_id"] is None

    def test_instance_stamped_with_book_id(self, auth_headers, fake_session):
        definition = _create_definition({**auth_headers, **BOOK_B})
        client.post(
            "/workflow-instances/",
            json={"workflow_definition_id": definition["id"], "triggered_by_event": "ev"},
            headers={**auth_headers, **BOOK_B},
        )
        assert fake_session.instances[0]["book_id"] == "book-bbb-222"

    def test_definition_hidden_from_other_book(self, auth_headers):
        created = _create_definition({**auth_headers, **BOOK_A})
        r = client.get(f"/workflow-definitions/{created['id']}", headers={**auth_headers, **BOOK_B})
        assert r.status_code == 404

    def test_definition_lists_scoped_to_book(self, auth_headers):
        _create_definition({**auth_headers, **BOOK_A}, name="Workflow A")
        _create_definition({**auth_headers, **BOOK_B}, name="Workflow B")

        r_a = client.get("/workflow-definitions/", headers={**auth_headers, **BOOK_A})
        assert [d["name"] for d in r_a.json()] == ["Workflow A"]
        r_b = client.get("/workflow-definitions/", headers={**auth_headers, **BOOK_B})
        assert [d["name"] for d in r_b.json()] == ["Workflow B"]

    def test_cross_book_update_blocked(self, auth_headers):
        created = _create_definition({**auth_headers, **BOOK_A})
        r = client.put(
            f"/workflow-definitions/{created['id']}", json={"name": "Hijacked"}, headers={**auth_headers, **BOOK_B}
        )
        assert r.status_code == 404

    def test_cross_book_delete_blocked(self, auth_headers, fake_session):
        created = _create_definition({**auth_headers, **BOOK_A})
        r = client.delete(f"/workflow-definitions/{created['id']}", headers={**auth_headers, **BOOK_B})
        assert r.status_code == 404
        assert len(fake_session.definitions) == 1  # still there

    def test_instance_cannot_attach_to_other_books_definition(self, auth_headers):
        definition = _create_definition({**auth_headers, **BOOK_A})
        r = client.post(
            "/workflow-instances/",
            json={"workflow_definition_id": definition["id"], "triggered_by_event": "ev"},
            headers={**auth_headers, **BOOK_B},
        )
        assert r.status_code == 404

    def test_cross_book_instance_update_blocked(self, auth_headers):
        definition = _create_definition({**auth_headers, **BOOK_A})
        r = client.post(
            "/workflow-instances/",
            json={"workflow_definition_id": definition["id"], "triggered_by_event": "ev"},
            headers={**auth_headers, **BOOK_A},
        )
        instance_id = r.json()["id"]
        r = client.put(
            f"/workflow-instances/{instance_id}", json={"status": "cancelled"}, headers={**auth_headers, **BOOK_B}
        )
        assert r.status_code == 404

    def test_unscoped_request_sees_all(self, auth_headers):
        """Template semantics: no X-Book-ID header = backwards-compatible unscoped view."""
        _create_definition(auth_headers, name="Personal")
        _create_definition({**auth_headers, **BOOK_B}, name="Book B")

        r = client.get("/workflow-definitions/", headers=auth_headers)
        assert len(r.json()) == 2  # unscoped sees personal + Book rows
