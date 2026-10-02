"""Shared fake Neo4j harness for all automation-engine test modules.

Both test files previously built their own FakeSession and patched
Neo4jConnector.get_driver at import time; the last patch won, so one
file's autouse cleanup cleared a session that no request ever used,
leaking rules across tests. One session + one autouse cleanup here.
"""
import importlib.util
import os

os.environ.setdefault("JWT_SECRET", "test-secret-key-for-testing-only")
os.environ.setdefault("NEO4J_PASSWORD", "test-password")

import pytest

_HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("ae_fake_shared", os.path.join(_HERE, "fake_neo4j.py"))
fake_module = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fake_module)

fake_session = fake_module.FakeSession()


@pytest.fixture(autouse=True)
def _clean_fake_graph():
    fake_session.nodes.clear()
    fake_session.edges.clear()
    yield
    fake_session.nodes.clear()
    fake_session.edges.clear()
