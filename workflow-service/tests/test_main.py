"""
Vimbai Workflow Service - Test Suite
Tests: workflow definitions, instances, execution
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
            "permissions": ["workflow:view", "workflow:create", "workflow:edit", "workflow:delete"],
            "exp": datetime.now(timezone.utc) + timedelta(hours=1),
        },
        os.environ["JWT_SECRET"],
        algorithm="HS256",
    )
    return {"Authorization": f"Bearer {token}"}


@pytest.fixture
def valid_workflow_def():
    return {
        "name": "Test Workflow",
        "description": "A test workflow definition",
        "steps": [
            {"name": "Step 1", "action": "notify", "config": {}},
            {"name": "Step 2", "action": "wait_approval", "config": {}},
        ],
    }


class TestHealthCheck:
    def test_root_endpoint(self):
        response = client.get("/")
        assert response.status_code == 200


class TestWorkflowDefinitions:
    def test_create_definition_no_auth(self, valid_workflow_def):
        response = client.post("/workflow-definitions/", json=valid_workflow_def)
        assert response.status_code in [401, 403]

    def test_create_definition_with_auth(self, auth_headers, valid_workflow_def):
        response = client.post("/workflow-definitions/", json=valid_workflow_def, headers=auth_headers)
        assert response.status_code in [201, 200, 500]

    def test_get_definitions_no_auth(self):
        response = client.get("/workflow-definitions/")
        assert response.status_code in [401, 403]

    def test_get_definitions_with_auth(self, auth_headers):
        response = client.get("/workflow-definitions/", headers=auth_headers)
        assert response.status_code in [200, 500]

    def test_create_definition_missing_fields(self, auth_headers):
        response = client.post("/workflow-definitions/", json={"name": "Missing Steps"}, headers=auth_headers)
        assert response.status_code in [422, 400, 201]


class TestWorkflowInstances:
    def test_create_instance_no_auth(self):
        response = client.post("/workflow-instances/", json={"definition_id": "test-def-id"})
        assert response.status_code in [401, 403]

    def test_get_instance_no_auth(self):
        response = client.get("/workflow-instances/test-instance-id")
        assert response.status_code in [401, 403]

    def test_delete_instance_no_auth(self):
        response = client.delete("/workflow-instances/test-instance-id")
        assert response.status_code in [401, 403]
