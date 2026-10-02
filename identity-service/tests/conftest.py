"""Test bootstrap: patch the shared fake Neo4j driver before the app loads."""

import importlib.util
import os

os.environ.setdefault("JWT_SECRET", "test-secret-key-for-testing-only")

import main  # noqa: E402  (self-bootstraps the identity_service package alias)
from identity_service.database import Neo4jConnector  # noqa: E402

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("identity_fake_neo4j", os.path.join(_HERE, "fake_neo4j.py"))
_fake_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_fake_mod)

_fake_session = _fake_mod.FakeSession()
Neo4jConnector.get_driver = classmethod(lambda cls: _fake_mod.FakeDriver(_fake_session))
