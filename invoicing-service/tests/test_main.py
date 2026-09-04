"""
Vimbai Invoicing Service - Test Suite
Tests: customer CRUD, invoice CRUD, Book isolation (X-Book-ID scoping).
"""

import os
import uuid
from datetime import datetime, timedelta, timezone

import pytest
from fastapi.testclient import TestClient

os.environ.setdefault("JWT_SECRET", "test-secret-key-for-testing-only")
os.environ.setdefault("NEO4J_PASSWORD", "test-password")

import invoicing_service.dependencies as deps
import invoicing_service.main as invoicing_main
from invoicing_service.dependencies import get_db_session
from main import app

client = TestClient(app)

BOOK_A = {"X-Book-ID": "book-aaa-111"}
BOOK_B = {"X-Book-ID": "book-bbb-222"}


# ---------------------------------------------------------------------------
# Fake Neo4j async session: interprets the exact Cypher patterns the CRUD uses.
# ---------------------------------------------------------------------------
class NeoStr(str):
    """String that mimics neo4j's DateTime .iso_format() interface."""

    def iso_format(self):
        return str(self)


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
        self.customers = []  # list of dicts
        self.invoices = []  # list of dicts
        self.items = []  # list of dicts, each with invoice_id
        self.edges = []  # (from_type, from_id, rel, to_type, to_id)

    @staticmethod
    def _visible(node, params):
        return params.get("book_id") is None or node.get("book_id") == params.get("book_id")

    async def run(self, query, params=None, **kw):
        merged = dict(params or {})
        merged.update(kw)

        # --- Customer CRUD ---
        if "CREATE (c:Customer $props)" in query:
            props = dict(merged["props"])
            self.customers.append(props)
            self.edges.append(("User", merged.get("user_id"), "OWNS_CUSTOMER", "Customer", props["id"]))
            return FakeResult([{"c": props}])

        if "CREATE (i:Invoice {" in query:
            customers = [
                c
                for c in self.customers
                if c["customer_id"] == merged["customer_id"]
                and c["user_id"] == merged["user_id"]
                and self._visible(c, merged)
            ]
            if not customers:
                return FakeResult()
            invoice = {
                "id": merged["id"],
                "book_id": merged.get("book_id"),
                "invoice_number": merged["invoice_number"],
                "invoice_date": NeoStr(merged["invoice_date"]),
                "due_date": NeoStr(merged["due_date"]),
                "total_amount": float(merged["total_amount"]),
                "status": merged["status"],
                "notes": merged.get("notes"),
                "created_at": NeoStr(merged["created_at"]),
                "updated_at": NeoStr(merged["updated_at"]),
            }
            self.invoices.append(invoice)
            self.edges.append(("Customer", customers[0]["id"], "HAS_INVOICE", "Invoice", invoice["id"]))
            return FakeResult([{"i": invoice}])

        if "CREATE (ii:InvoiceItem {" in query:
            item = {
                "id": merged["id"],
                "description": merged["description"],
                "quantity": float(merged["quantity"]),
                "unit_price": float(merged["unit_price"]),
                "amount": float(merged["amount"]),
                "account_number": merged["account_number"],
                "created_at": NeoStr(merged["created_at"]),
                "updated_at": NeoStr(merged["updated_at"]),
                "invoice_id": merged["invoice_id"],
            }
            self.items.append(item)
            return FakeResult([{"ii": item}])

        if "MATCH (c:Customer {customer_id: $customer_id, user_id: $user_id})" in query:
            matches = [
                c
                for c in self.customers
                if c["customer_id"] == merged["customer_id"]
                and c["user_id"] == merged["user_id"]
                and self._visible(c, merged)
            ]
            if "DETACH DELETE c" in query:
                deleted = len(matches)
                for c in matches:
                    self.customers.remove(c)
                return FakeResult(nodes_deleted=deleted)
            if "SET" in query:
                for c in matches:
                    for key in ("name", "email", "phone", "address", "updated_at"):
                        if key in merged:
                            c[key] = merged[key]
                    return FakeResult([{"c": c}])
            if matches:
                return FakeResult([{"c": matches[0]}])
            return FakeResult()

        if "MATCH (c:Customer {user_id: $user_id})" in query and "ORDER BY c.name" in query:
            matches = [c for c in self.customers if c["user_id"] == merged["user_id"] and self._visible(c, merged)]
            matches.sort(key=lambda c: c.get("name", ""))
            return FakeResult([{"c": c} for c in matches])

        # --- Invoice CRUD ---
        if "DETACH DELETE i, ii" in query:
            invs = [
                i for i in self.invoices if i["invoice_number"] == merged["invoice_number"] and self._visible(i, merged)
            ]
            deleted = len(invs)
            for inv in invs:
                self.invoices.remove(inv)
                self.items = [it for it in self.items if it["invoice_id"] != inv["id"]]
            return FakeResult(nodes_deleted=deleted)

        if (
            "MATCH (c:Customer {user_id: $user_id})-[:HAS_INVOICE]->(i:Invoice {invoice_number: $invoice_number})"
            in query
            and "OPTIONAL MATCH (i)-[:HAS_ITEM]->(ii:InvoiceItem)" in query
        ):
            invs = [
                i for i in self.invoices if i["invoice_number"] == merged["invoice_number"] and self._visible(i, merged)
            ]
            if not invs:
                return FakeResult()
            inv = invs[0]
            edge = next(
                (e for e in self.edges if e[3] == "Invoice" and e[4] == inv["id"] and e[2] == "HAS_INVOICE"),
                None,
            )
            customer_id = None
            if edge:
                cust = next((c for c in self.customers if c["id"] == edge[1]), None)
                customer_id = cust["customer_id"] if cust else None
            items = [it for it in self.items if it["invoice_id"] == inv["id"]]
            return FakeResult([{"i": inv, "items": items, "customer_id": customer_id}])

        if (
            "MATCH (c:Customer {user_id: $user_id})-[:HAS_INVOICE]->(i:Invoice)" in query
            and "ORDER BY i.invoice_date" in query
        ):
            invs = [i for i in self.invoices if self._visible(i, merged)]
            records = []
            for inv in invs:
                edge = next(
                    (e for e in self.edges if e[3] == "Invoice" and e[4] == inv["id"] and e[2] == "HAS_INVOICE"),
                    None,
                )
                customer_id = None
                if edge:
                    cust = next((c for c in self.customers if c["id"] == edge[1]), None)
                    customer_id = cust["customer_id"] if cust else None
                items = [it for it in self.items if it["invoice_id"] == inv["id"]]
                records.append({"i": inv, "items": items, "customer_id": customer_id})
            return FakeResult(records)

        if (
            "SET {set_query_part}".format(set_query_part="") in query
            or "SET i." in query
            or ("SET" in query and "RETURN i" in query)
        ):
            # update_invoice: dynamic SET clause
            invs = [
                i for i in self.invoices if i["invoice_number"] == merged["invoice_number"] and self._visible(i, merged)
            ]
            if not invs:
                return FakeResult()
            inv = invs[0]
            for key in ("invoice_date", "due_date", "total_amount", "status", "notes", "updated_at"):
                if key in merged:
                    inv[key] = NeoStr(merged[key]) if key in ("invoice_date", "due_date", "updated_at") else merged[key]
            return FakeResult([{"i": inv}])

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
            "permissions": ["*.*"],
            "exp": datetime.now(timezone.utc) + timedelta(hours=1),
        },
        os.environ["JWT_SECRET"],
        algorithm="HS256",
    )
    return {"Authorization": f"Bearer {token}"}


def _customer_payload(customer_id="CUST-001"):
    return {"name": "Acme Corp", "email": "contact@acme.com", "customer_id": customer_id}


def _invoice_payload(customer_id="CUST-001", invoice_number="INV-001"):
    return {
        "customer_id": customer_id,
        "invoice_number": invoice_number,
        "invoice_date": "2026-09-01T10:00:00",
        "due_date": "2026-09-30T10:00:00",
        "total_amount": "150.00",
        "status": "draft",
        "items": [
            {
                "description": "Consulting services",
                "quantity": "3",
                "unit_price": "50.00",
                "amount": "150.00",
                "account_number": "4000",
            }
        ],
    }


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200


class TestAuth:
    def test_create_customer_no_auth(self):
        response = client.post("/customers/", json=_customer_payload())
        assert response.status_code in [401, 403]

    def test_list_customers_no_auth(self):
        response = client.get("/customers/")
        assert response.status_code in [401, 403]


class TestCustomerCRUD:
    def test_create_and_get_customer(self, auth_headers, fake_session):
        r = client.post("/customers/", json=_customer_payload(), headers=auth_headers)
        assert r.status_code == 201, r.text
        assert r.json()["customer_id"] == "CUST-001"

        r = client.get("/customers/CUST-001", headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["name"] == "Acme Corp"

    def test_duplicate_customer_rejected(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers=auth_headers)
        r = client.post("/customers/", json=_customer_payload(), headers=auth_headers)
        assert r.status_code == 409

    def test_update_customer(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers=auth_headers)
        r = client.put("/customers/CUST-001", json={"name": "Acme Corp Ltd"}, headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["name"] == "Acme Corp Ltd"

    def test_delete_customer(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers=auth_headers)
        r = client.delete("/customers/CUST-001", headers=auth_headers)
        assert r.status_code == 204

    def test_unknown_customer_404(self, auth_headers):
        r = client.get("/customers/NOPE-404", headers=auth_headers)
        assert r.status_code == 404


class TestInvoiceCRUD:
    def test_create_and_get_invoice(self, auth_headers, fake_session):
        client.post("/customers/", json=_customer_payload(), headers=auth_headers)
        r = client.post("/invoices/", json=_invoice_payload(), headers=auth_headers)
        assert r.status_code == 201, r.text
        body = r.json()
        assert body["invoice_number"] == "INV-001"
        from decimal import Decimal

        assert Decimal(body["total_amount"]) == Decimal("150.00")
        assert len(body["items"]) == 1

        r = client.get("/invoices/INV-001", headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["status"] == "draft"

    def test_list_invoices(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers=auth_headers)
        client.post("/invoices/", json=_invoice_payload(), headers=auth_headers)
        r = client.get("/invoices/", headers=auth_headers)
        assert r.status_code == 200
        assert len(r.json()) == 1

    def test_update_invoice_status(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers=auth_headers)
        client.post("/invoices/", json=_invoice_payload(), headers=auth_headers)
        r = client.put("/invoices/INV-001", json={"status": "outstanding"}, headers=auth_headers)
        assert r.status_code == 200
        assert r.json()["status"] == "outstanding"

    def test_delete_invoice(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers=auth_headers)
        client.post("/invoices/", json=_invoice_payload(), headers=auth_headers)
        r = client.delete("/invoices/INV-001", headers=auth_headers)
        assert r.status_code == 204

    def test_unknown_invoice_404(self, auth_headers):
        r = client.get("/invoices/NOPE-404", headers=auth_headers)
        assert r.status_code == 404


class TestBookIsolation:
    def test_customer_stamped_with_book_id(self, auth_headers, fake_session):
        client.post("/customers/", json=_customer_payload(), headers={**auth_headers, **BOOK_A})
        stored = fake_session.customers[0]
        assert stored["book_id"] == "book-aaa-111"

        # unscoped (personal) requests keep book_id = None
        client.post("/customers/", json=_customer_payload("CUST-002"), headers=auth_headers)
        assert fake_session.customers[1]["book_id"] is None

    def test_invoice_stamped_with_book_id(self, auth_headers, fake_session):
        client.post("/customers/", json=_customer_payload(), headers={**auth_headers, **BOOK_A})
        client.post("/invoices/", json=_invoice_payload(), headers={**auth_headers, **BOOK_A})
        stored = fake_session.invoices[0]
        assert stored["book_id"] == "book-aaa-111"

    def test_customer_hidden_from_other_book(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers={**auth_headers, **BOOK_A})
        r = client.get("/customers/CUST-001", headers={**auth_headers, **BOOK_B})
        assert r.status_code == 404

    def test_invoice_hidden_from_other_book(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers={**auth_headers, **BOOK_A})
        client.post("/invoices/", json=_invoice_payload(), headers={**auth_headers, **BOOK_A})
        r = client.get("/invoices/INV-001", headers={**auth_headers, **BOOK_B})
        assert r.status_code == 404

    def test_customer_lists_scoped_to_book(self, auth_headers):
        client.post("/customers/", json=_customer_payload("CUST-A"), headers={**auth_headers, **BOOK_A})
        client.post("/customers/", json=_customer_payload("CUST-B"), headers={**auth_headers, **BOOK_B})

        r_a = client.get("/customers/", headers={**auth_headers, **BOOK_A})
        assert [c["customer_id"] for c in r_a.json()] == ["CUST-A"]
        r_b = client.get("/customers/", headers={**auth_headers, **BOOK_B})
        assert [c["customer_id"] for c in r_b.json()] == ["CUST-B"]

    def test_invoice_lists_scoped_to_book(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers={**auth_headers, **BOOK_A})
        client.post("/customers/", json=_customer_payload("CUST-002"), headers={**auth_headers, **BOOK_B})
        client.post("/invoices/", json=_invoice_payload(invoice_number="INV-A"), headers={**auth_headers, **BOOK_A})
        client.post(
            "/invoices/",
            json=_invoice_payload(customer_id="CUST-002", invoice_number="INV-B"),
            headers={**auth_headers, **BOOK_B},
        )

        r_a = client.get("/invoices/", headers={**auth_headers, **BOOK_A})
        assert [i["invoice_number"] for i in r_a.json()] == ["INV-A"]
        r_b = client.get("/invoices/", headers={**auth_headers, **BOOK_B})
        assert [i["invoice_number"] for i in r_b.json()] == ["INV-B"]

    def test_same_invoice_number_allowed_in_different_books(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers={**auth_headers, **BOOK_A})
        client.post("/customers/", json=_customer_payload("CUST-002"), headers={**auth_headers, **BOOK_B})
        r1 = client.post(
            "/invoices/", json=_invoice_payload(invoice_number="INV-SHARED"), headers={**auth_headers, **BOOK_A}
        )
        r2 = client.post(
            "/invoices/",
            json=_invoice_payload(customer_id="CUST-002", invoice_number="INV-SHARED"),
            headers={**auth_headers, **BOOK_B},
        )
        assert r1.status_code == 201, r1.text
        assert r2.status_code == 201, r2.text

    def test_cross_book_update_blocked(self, auth_headers):
        client.post("/customers/", json=_customer_payload(), headers={**auth_headers, **BOOK_A})
        client.post("/invoices/", json=_invoice_payload(), headers={**auth_headers, **BOOK_A})
        r = client.put("/invoices/INV-001", json={"status": "void"}, headers={**auth_headers, **BOOK_B})
        assert r.status_code == 404

    def test_cross_book_delete_blocked(self, auth_headers, fake_session):
        client.post("/customers/", json=_customer_payload(), headers={**auth_headers, **BOOK_A})
        client.post("/invoices/", json=_invoice_payload(), headers={**auth_headers, **BOOK_A})
        r = client.delete("/invoices/INV-001", headers={**auth_headers, **BOOK_B})
        assert r.status_code == 404
        assert len(fake_session.invoices) == 1  # still there

    def test_unscoped_request_sees_all(self, auth_headers):
        """Template semantics: no X-Book-ID header = backwards-compatible unscoped view."""
        client.post("/customers/", json=_customer_payload(), headers=auth_headers)  # personal (book_id None)
        client.post("/customers/", json=_customer_payload("CUST-B"), headers={**auth_headers, **BOOK_B})

        r = client.get("/customers/", headers=auth_headers)
        ids = sorted(c["customer_id"] for c in r.json())
        assert ids == ["CUST-001", "CUST-B"]  # unscoped sees personal + Book rows

        # but a Book-scoped request only sees its own
        r_b = client.get("/customers/", headers={**auth_headers, **BOOK_B})
        assert [c["customer_id"] for c in r_b.json()] == ["CUST-B"]
