import os
import sys

import pytest
from fastapi.testclient import TestClient

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from main import app

client = TestClient(app)


def test_health_check():
    response = client.get("/health")
    assert response.status_code == 200
    assert response.json()["status"] == "healthy"


def test_register_backup_success():
    payload = {
        "user_id": "user_123",
        "backup_id": "backup_abc",
        "filename": "Vimbai_Backup_2026.vmb",
        "version": "1.0",
        "integrity_signature": "sig_123",
        "account_binding_info": "crypto_id_123",
        "storage_provider": "google_drive",
        "timestamp": "2026-07-18T10:00:00Z",
    }
    response = client.post("/backups/register", json=payload, headers={"authorization": "Bearer token"})
    assert response.status_code == 200
    assert response.json()["backup_id"] == "backup_abc"


def test_register_backup_unauthorized():
    response = client.post(
        "/backups/register",
        json={
            "user_id": "123",
            "backup_id": "abc",
            "filename": "x",
            "version": "1",
            "integrity_signature": "s",
            "account_binding_info": "b",
            "storage_provider": "local",
            "timestamp": "t",
        },
    )
    assert response.status_code == 401


def test_verify_backup_binding_success():
    # Register first
    client.post(
        "/backups/register",
        json={
            "user_id": "user_456",
            "backup_id": "backup_def",
            "filename": "v.vmb",
            "version": "1",
            "integrity_signature": "s",
            "account_binding_info": "secret_bind_456",
            "storage_provider": "local",
            "timestamp": "t",
        },
        headers={"authorization": "Bearer token"},
    )

    response = client.post(
        "/backups/verify-binding?user_id=user_456&backup_id=backup_def&provided_binding_info=secret_bind_456"
    )
    assert response.status_code == 200
    assert response.json()["status"] == "success"


def test_verify_backup_binding_failure():
    response = client.post(
        "/backups/verify-binding?user_id=user_456&backup_id=backup_def&provided_binding_info=wrong_bind"
    )
    assert response.status_code == 403
