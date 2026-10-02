"""Smoke test: app imports and health responds."""

import main
from fastapi.testclient import TestClient

client = TestClient(main.app)


def test_root_and_health():
    assert client.get("/").status_code == 200
    assert client.get("/health").json()["status"] == "healthy"
