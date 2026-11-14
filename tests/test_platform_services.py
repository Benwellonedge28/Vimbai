"""
Integration tests for Policy Engine, Webhook, WebSocket, State Machine,
and other platform services.
"""

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service

_H = {"X-User-Id": "root-platform-user"}


def _patch_fake(pkg_name, fake_name):
    """Patch a converted service's Neo4j connector onto the fake harness.

    Binds ONE shared fake session (a fresh session per request would lose
    writes between calls).
    """
    import importlib
    import importlib.util
    import os
    import sys
    from types import ModuleType

    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    fake_path = os.path.join(repo_root, pkg_name.replace("_", "-"), "fake_neo4j.py")
    spec = importlib.util.spec_from_file_location(fake_name, fake_path)
    fake = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fake)
    sys.modules[fake_name] = fake
    if pkg_name not in sys.modules or not hasattr(sys.modules.get(pkg_name), "__path__"):
        alias = ModuleType(pkg_name)
        alias.__path__ = [os.path.join(repo_root, pkg_name.replace("_", "-"))]
        sys.modules[pkg_name] = alias
    db = importlib.import_module(f"{pkg_name}.database")
    session = fake.FakeSession()
    db.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(session))


