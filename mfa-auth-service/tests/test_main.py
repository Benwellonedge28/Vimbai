import pytest
from fastapi.testclient import TestClient
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))
from main import app

client = TestClient(app)

def test_health_check():
    response = client.get("/health")
    assert response.status_code == 200

def test_mfa_backup_restore_success():
    response = client.post("/auth/verify", json={
        "user_id": "u1",
        "action_type": "restore_backup",
        "knowledge_factor": "password_hash",
        "biometric_token": "android_bio_token"
    })
    assert response.status_code == 200
    assert response.json()["authorized"] == True

def test_mfa_backup_restore_with_recovery_phrase():
    response = client.post("/auth/verify", json={
        "user_id": "u1",
        "action_type": "restore_backup",
        "knowledge_factor": "recovery_phrase_12_words"
    })
    assert response.status_code == 200
    assert response.json()["authorized"] == True

def test_mfa_backup_restore_insufficient_factors():
    response = client.post("/auth/verify", json={
        "user_id": "u1",
        "action_type": "restore_backup",
        "knowledge_factor": "password_hash"
    })
    assert response.status_code == 403

def test_mfa_approve_expense_success():
    response = client.post("/auth/verify", json={
        "user_id": "u1",
        "action_type": "approve_expense",
        "biometric_token": "bio_token"
    })
    assert response.status_code == 200
    assert response.json()["authorized"] == True
