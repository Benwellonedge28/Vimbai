"""
Integration tests for Treasury Management, Compliance, and Analytics services.
"""

import importlib.util
import os
import sys
from decimal import Decimal

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service

TM_HEADERS = {"X-User-Id": "root-tm-user"}


@pytest.fixture
def compliance_client():
    svc = load_service("treasury-compliance-service")
    app = svc.main.app
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    fake_path = os.path.join(repo_root, "treasury-compliance-service", "fake_neo4j.py")
    spec = importlib.util.spec_from_file_location("tc_root_fake", fake_path)
    fake_mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fake_mod)
    session = fake_mod.FakeSession()
    # conftest replaces the parent alias with a bare module (no __path__), so
    # load the lazily-imported database module by file path and patch it
    db_spec = importlib.util.spec_from_file_location(
        "treasury_compliance_service.database", os.path.join(os.path.dirname(fake_path), "database.py")
    )
    db_mod = importlib.util.module_from_spec(db_spec)
    sys.modules["treasury_compliance_service.database"] = db_mod
    db_spec.loader.exec_module(db_mod)
    db_mod.Neo4jConnector.get_driver = classmethod(lambda cls: fake_mod.FakeDriver(session))
    client = TestClient(app)
    client.headers.update(TM_HEADERS)
    return client
