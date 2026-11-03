"""Encrypted backups: Book-scoped Neo4j persistence.

Backup and restore jobs were module-level lists: gone on restart, shared by
every tenant, so anyone could restore from another company's backup by id.
"""

import encrypted_backup_store as store
import pytest
from fake_neo4j import FakeDriver, FakeSession
from fastapi.testclient import TestClient
from main import app

BOOK_ONE = {"X-User-Id": "user-1", "X-Book-ID": "book-1"}
BOOK_TWO = {"X-User-Id": "user-2", "X-Book-ID": "book-2"}


@pytest.fixture
def client():
    store.set_driver(FakeDriver(FakeSession()))
    yield TestClient(app)
    store.set_driver(None)


def _backup(client, service_name="ledger-service", headers=BOOK_ONE, backup_type="full"):
    resp = client.post(
        "/backup",
        json={"service_name": service_name, "backup_type": backup_type, "created_by": "ops"},
        headers=headers,
    )
    assert resp.status_code == 200, resp.text
    return resp.json()


def test_health(client):
    assert client.get("/health").json()["service"] == "encrypted-backup-service"


def test_backup_completes_and_round_trips(client):
    job = _backup(client)
    assert job["status"] == "completed"
    assert job["checksum"].startswith("sha256:")
    fetched = client.get(f"/backups/{job['id']}", headers=BOOK_ONE)
    assert fetched.status_code == 200
    assert fetched.json()["file_path"].endswith(".enc")
    assert client.get("/backups", headers=BOOK_ONE).json()[0]["id"] == job["id"]


def test_invalid_backup_type_is_rejected(client):
    resp = client.post("/backup", json={"service_name": "x", "backup_type": "snapshot"}, headers=BOOK_ONE)
    assert resp.status_code == 400


def test_restore_is_recorded(client):
    job = _backup(client)
    restore = client.post(f"/backup/{job['id']}/restore", params={"restored_by": "ops"}, headers=BOOK_ONE)
    assert restore.status_code == 200, restore.text
    assert restore.json()["status"] == "completed"
    restores = client.get("/restores", headers=BOOK_ONE).json()
    assert [r["backup_id"] for r in restores] == [job["id"]]


def test_delete_removes_the_backup(client):
    job = _backup(client)
    assert client.delete(f"/backups/{job['id']}", headers=BOOK_ONE).json()["deleted"] is True
    assert client.get(f"/backups/{job['id']}", headers=BOOK_ONE).status_code == 404


def test_one_book_can_neither_read_nor_restore_another_books_backup(client):
    job = _backup(client, headers=BOOK_ONE)
    assert client.get("/backups", headers=BOOK_TWO).json() == []
    assert client.get(f"/backups/{job['id']}", headers=BOOK_TWO).status_code == 404
    assert client.post(f"/backup/{job['id']}/restore", headers=BOOK_TWO).status_code == 404
    assert client.delete(f"/backups/{job['id']}", headers=BOOK_TWO).status_code == 404
    assert client.get("/backups", headers=BOOK_ONE).json() != []


def test_service_name_filter(client):
    _backup(client, "ledger-service")
    other = _backup(client, "payroll-service")
    filtered = client.get("/backups", params={"service_name": "payroll-service"}, headers=BOOK_ONE).json()
    assert [b["id"] for b in filtered] == [other["id"]]
