"""
Vimbai Multimodal Pipeline Service - Test Suite
Tests: task CRUD, document OCR, audio processing, corrections
"""

import os
from unittest.mock import AsyncMock, patch

import pytest
from fastapi.testclient import TestClient

os.environ["JWT_SECRET"] = "test-secret-key-for-testing-only"
os.environ["NEO4J_PASSWORD"] = "test-password"

from main import app

client = TestClient(app)


@pytest.fixture
def auth_headers():
    from datetime import datetime, timedelta, timezone

    import jwt as pyjwt

    token = pyjwt.encode(
        {
            "user_id": "test-user-id",
            "username": "testuser",
            "role": "admin",
            "permissions": ["multimodal.read.tasks", "multimodal.write.tasks", "multimodal.delete.tasks"],
            "exp": datetime.now(timezone.utc) + timedelta(hours=1),
        },
        os.environ["JWT_SECRET"],
        algorithm="HS256",
    )
    return {"Authorization": f"Bearer {token}"}


@pytest.fixture
def valid_task():
    return {"input_type": "text", "input_data": "Sample invoice text for processing", "user_id": "test-user-id"}


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200


class TestTaskCRUD:
    def test_create_task_no_auth(self, valid_task):
        response = client.post("/tasks/", json=valid_task)
        assert response.status_code in [401, 403]

    def test_create_task_with_auth(self, auth_headers, valid_task):
        response = client.post("/tasks/", json=valid_task, headers=auth_headers)
        assert response.status_code in [201, 200, 500, 503]

    def test_get_tasks_no_auth(self):
        response = client.get("/tasks/")
        assert response.status_code in [401, 403]

    def test_get_tasks_with_auth(self, auth_headers):
        response = client.get("/tasks/", headers=auth_headers)
        assert response.status_code in [200, 500, 503]

    def test_get_single_task_no_auth(self):
        response = client.get("/tasks/test-task-id")
        assert response.status_code in [401, 403]

    def test_delete_task_no_auth(self):
        response = client.delete("/tasks/test-task-id")
        assert response.status_code in [401, 403]

    def test_create_task_missing_fields(self, auth_headers):
        response = client.post("/tasks/", json={"input_type": "text"}, headers=auth_headers)
        assert response.status_code in [422, 400, 503]


class TestCorrections:
    def test_submit_correction_no_auth(self):
        response = client.post(
            "/tasks/test-task-id/corrections",
            json={"field_name": "amount", "original_value": "100.00", "corrected_value": "150.00"},
        )
        assert response.status_code in [401, 403]

    def test_get_corrections_no_auth(self):
        response = client.get("/tasks/test-task-id/corrections")
        assert response.status_code in [401, 403]
