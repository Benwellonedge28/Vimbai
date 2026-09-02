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


def test_rbac_cfo_approve_large_expense():
    response = client.post(
        "/authorize",
        json={
            "user": {"user_id": "u1", "role": "CFO", "department": "Finance", "location": "HQ"},
            "action": "approve",
            "resource": {"resource_type": "expense", "amount": 50000, "department": "Sales", "project": "P1"},
        },
    )
    assert response.status_code == 200
    assert response.json()["authorized"] == True


def test_rbac_manager_cannot_approve_large_expense():
    response = client.post(
        "/authorize",
        json={
            "user": {"user_id": "u2", "role": "Department Manager", "department": "Sales", "location": "HQ"},
            "action": "approve",
            "resource": {"resource_type": "expense", "amount": 50000, "department": "Sales", "project": "P1"},
        },
    )
    assert response.status_code == 200
    data = response.json()
    assert data["authorized"] == False
    assert "CFO" in data["required_approvals"]


def test_abac_manager_cannot_approve_other_department():
    response = client.post(
        "/authorize",
        json={
            "user": {"user_id": "u3", "role": "Department Manager", "department": "HR", "location": "HQ"},
            "action": "approve",
            "resource": {"resource_type": "expense", "amount": 1000, "department": "Sales", "project": "P1"},
        },
    )
    assert response.status_code == 200
    assert response.json()["authorized"] == False
    assert "own department" in response.json()["reason"]


def test_abac_manager_can_approve_own_department():
    response = client.post(
        "/authorize",
        json={
            "user": {"user_id": "u4", "role": "Department Manager", "department": "Sales", "location": "HQ"},
            "action": "approve",
            "resource": {"resource_type": "expense", "amount": 1000, "department": "Sales", "project": "P1"},
        },
    )
    assert response.status_code == 200
    assert response.json()["authorized"] == True
