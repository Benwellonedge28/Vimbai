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


def test_push_encrypted_data_success():
    response = client.post(
        "/sync/push",
        json={
            "user_id": "user_001",
            "data_type": "budget",
            "encrypted_payload": "AES256_ENCRYPTED_BLOB_XYZ",
            "iv": "random_iv_value",
            "auth_tag": "auth_tag_value",
        },
        headers={"authorization": "Bearer test_token"},
    )
    assert response.status_code == 200
    assert response.json()["status"] == "success"
    assert "Server cannot read" in response.json()["message"]


def test_push_requires_auth():
    response = client.post(
        "/sync/push",
        json={"user_id": "user_001", "data_type": "budget", "encrypted_payload": "blob", "iv": "iv", "auth_tag": "tag"},
    )
    assert response.status_code == 401


def test_pull_encrypted_data_success():
    # Push first
    client.post(
        "/sync/push",
        json={
            "user_id": "user_002",
            "data_type": "transaction",
            "encrypted_payload": "ENCRYPTED_TRANSACTION_BLOB",
            "iv": "iv_val",
            "auth_tag": "tag_val",
        },
        headers={"authorization": "Bearer test_token"},
    )

    response = client.get("/sync/pull/user_002", headers={"authorization": "Bearer test_token"})
    assert response.status_code == 200
    data = response.json()
    assert data["user_id"] == "user_002"
    assert "Decryption must occur on-device" in data["note"]


def test_pull_requires_auth():
    response = client.get("/sync/pull/user_001")
    assert response.status_code == 401
