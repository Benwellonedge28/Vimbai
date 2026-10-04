"""Tests for the Vimbai local (on-device, offline) runtime.

Covers the offline promises: single-process boot, gateway-parity auth,
Book-scoped isolation, and durability across a full restart (the store is
rebuilt from the data dir, nothing in memory).
"""

import importlib
import os
import uuid

import pytest
from fastapi.testclient import TestClient

os.environ.setdefault("JWT_SECRET", "local-test-secret")

from vimbai_local.launcher import build_app  # noqa: E402


@pytest.fixture()
def local_stack(tmp_path):
    data_dir = str(tmp_path / "vimbai-data")
    app, loaded, skipped, persisted = build_app(profile="core", data_dir=data_dir, port=8000)
    client = TestClient(app)
    return client, data_dir


def _register_and_login(client, tag):
    suffix = uuid.uuid4().hex[:8]
    username = f"local_{tag}_{suffix}"
    reg = client.post(
        "/identity/users/register",
        json={
            "email": f"{username}@example.com",
            "username": username,
            "password": "Local-Pass!2026",
            "first_name": "Local",
            "last_name": tag,
        },
    )
    assert reg.status_code == 201, reg.text
    login = client.post("/identity/users/login", data={"username": username, "password": "Local-Pass!2026"})
    assert login.status_code == 200, login.text
    token = login.json()["access_token"]
    return {"Authorization": f"Bearer {token}"}, username


def _create_book(client, auth):
    book = client.post(
        "/book-sync/books",
        json={"name": "Local Book", "tier": "business"},
        headers=auth,
    )
    assert book.status_code == 200, book.text
    book_id = (book.json().get("book") or {}).get("id") or book.json().get("id")
    assert book_id
    return book_id


def test_health_reports_core_profile(local_stack):
    client, _ = local_stack
    resp = client.get("/health")
    assert resp.status_code == 200
    body = resp.json()
    assert body["status"] == "healthy"
    assert body["profile"] == "core"
    assert body["services_loaded"] == 5
    assert body["services_skipped"] == 0
    assert body["skipped"] == []


def test_unauthenticated_call_rejected(local_stack):
    client, _ = local_stack
    resp = client.get("/departmental-accounting/departments")
    assert resp.status_code == 401


def test_book_scoped_write_read_and_isolation(local_stack):
    client, _ = local_stack
    auth_a, _ = _register_and_login(client, "a")
    auth_b, _ = _register_and_login(client, "b")
    book_id = _create_book(client, auth_a)

    headers_a = {**auth_a, "X-Book-ID": book_id}
    created = client.post(
        "/departmental-accounting/departments",
        json={
            "id": "loc-sales-1",
            "name": "Sales",
            "department_code": "LOC-1",
            "department_name": "Sales",
            "department_type": "revenue",
            "manager_id": "mgr-1",
            "manager_name": "Sam",
        },
        headers=headers_a,
    )
    assert created.status_code in (200, 201), created.text
    dept_id = created.json().get("id")

    listing = client.get("/departmental-accounting/departments", headers=headers_a)
    assert listing.status_code == 200
    items = listing.json() if isinstance(listing.json(), list) else listing.json().get("departments", [])
    assert dept_id in [d.get("id") for d in items]

    # User B sees none of user A's data and cannot use A's Book.
    listing_b = client.get("/departmental-accounting/departments", headers=auth_b)
    assert listing_b.status_code == 200
    b_items = listing_b.json() if isinstance(listing_b.json(), list) else listing_b.json().get("departments", [])
    assert dept_id not in [d.get("id") for d in b_items]

    stolen = client.get("/departmental-accounting/departments", headers={**auth_b, "X-Book-ID": book_id})
    assert stolen.status_code == 403


def test_data_survives_full_restart(local_stack):
    client, data_dir = local_stack
    auth_a, username = _register_and_login(client, "a")
    book_id = _create_book(client, auth_a)
    headers_a = {**auth_a, "X-Book-ID": book_id}
    created = client.post(
        "/departmental-accounting/departments",
        json={
            "id": "loc-durable-1",
            "name": "Durable",
            "department_code": "DUR-1",
            "department_name": "Durable",
            "department_type": "cost",
            "manager_id": "mgr-2",
            "manager_name": "Sam",
        },
        headers=headers_a,
    )
    assert created.status_code in (200, 201), created.text
    dept_id = created.json()["id"]

    # --- simulate a machine restart: rebuild everything from the data dir ---
    app2, loaded, skipped, _ = build_app(profile="core", data_dir=data_dir, port=8000)
    assert loaded and not skipped
    client2 = TestClient(app2)

    # Same user can log in again (identity store persisted).
    login = client2.post("/identity/users/login", data={"username": username, "password": "Local-Pass!2026"})
    assert login.status_code == 200, login.text
    auth2 = {"Authorization": f"Bearer {login.json()['access_token']}"}

    # Book still there.
    books = client2.get("/book-sync/books", headers=auth2)
    assert books.status_code == 200
    assert book_id in [(b.get("book") or {}).get("id") or b.get("id") for b in _book_list(books.json())]

    # Department still there under the same Book.
    listing = client2.get("/departmental-accounting/departments", headers={**auth2, "X-Book-ID": book_id})
    assert listing.status_code == 200
    items = listing.json() if isinstance(listing.json(), list) else listing.json().get("departments", [])
    assert dept_id in [d.get("id") for d in items]


def _book_list(payload):
    if isinstance(payload, list):
        return payload
    if isinstance(payload, dict):
        for key in ("books", "items", "data"):
            if isinstance(payload.get(key), list):
                return payload[key]
    return []
