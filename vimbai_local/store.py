"""File-backed persistence for Vimbai's local (on-device) runtime.

Each durable service ships a `fake_neo4j.py` driver whose FakeSession
implements exactly the Cypher subset that service uses (proven by the repo's
service test suites). In local mode we wrap THAT session class so every
mutating query is snapshotted to a per-service JSON file in the data dir and
restored on boot. No server, no JVM, no network - just a file on disk.
"""

import importlib.util
import json
import os
import re
import threading

_MUTATION_RE = re.compile(r"\b(CREATE|SET|DELETE|DETACH|MERGE|REMOVE)\b", re.IGNORECASE)


def _sanitize(value):
    """Make a prop value JSON-safe."""
    if hasattr(value, "iso_format"):  # Temporal shim
        return {"__temporal__": value.iso_format()}
    if hasattr(value, "isoformat"):  # datetime/date
        return {"__datetime__": value.isoformat()}
    if isinstance(value, (list, tuple)):
        return [_sanitize(v) for v in value]
    if isinstance(value, dict):
        return {str(k): _sanitize(v) for k, v in value.items()}
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return str(value)


def _revive(value):
    if isinstance(value, dict):
        if "__temporal__" in value:
            return _Temporal(value["__temporal__"])
        if "__datetime__" in value:
            return _Temporal(value["__datetime__"])
        return {k: _revive(v) for k, v in value.items()}
    if isinstance(value, list):
        return [_revive(v) for v in value]
    return value


class _Temporal:
    """Revived Temporal-compatible value (iso_format + str)."""

    def __init__(self, iso):
        self._iso = iso

    def iso_format(self):
        return self._iso

    def __str__(self):
        return self._iso


class ServiceStore:
    """Per-service JSON snapshot store."""

    def __init__(self, path):
        self.path = path
        self._lock = threading.Lock()

    def save(self, session):
        with self._lock:
            nodes = []
            for n in session.nodes:
                nodes.append({"label": n["label"], "var": n["var"], "props": _sanitize(n["props"])})
            index = {id(n): i for i, n in enumerate(session.nodes)}
            edges = [
                {"rel": e[0], "user_id": e[1], "node": index.get(id(e[2]))}
                for e in session.edges
                if index.get(id(e[2])) is not None
            ]
            tmp = self.path + ".tmp"
            with open(tmp, "w", encoding="utf-8") as fh:
                json.dump({"nodes": nodes, "edges": edges}, fh)
            os.replace(tmp, self.path)

    def load(self, session):
        if not os.path.isfile(self.path):
            return False
        with open(self.path, encoding="utf-8") as fh:
            data = json.load(fh)
        session.nodes = [
            {"label": n["label"], "var": n["var"], "props": _revive(n["props"])} for n in data.get("nodes", [])
        ]
        session.edges = [(e["rel"], e["user_id"], session.nodes[e["node"]]) for e in data.get("edges", [])]
        return True


def attach_service_store(service_dir, service_pkg, data_dir):
    """Patch <service_pkg>.database.Neo4jConnector to use a persisted fake.

    Returns True if the service was patched, False if it has no
    fake_neo4j.py (stateless / sqlite services need no patching).
    """
    fake_path = os.path.join(service_dir, "fake_neo4j.py")
    if not os.path.isfile(fake_path):
        return False
    db = importlib.import_module(f"{service_pkg}.database")
    fake_name = f"vimbai_local_fake_{service_pkg}"
    if fake_name in __import__("sys").modules:
        fake = __import__("sys").modules[fake_name]
    else:
        spec = importlib.util.spec_from_file_location(fake_name, fake_path)
        fake = importlib.util.module_from_spec(spec)
        __import__("sys").modules[fake_name] = fake
        spec.loader.exec_module(fake)

    store_path = os.path.join(data_dir, f"{service_pkg}.json")
    store = ServiceStore(store_path)

    class _PersistentSession(fake.FakeSession):
        _base_run = fake.FakeSession.run

        async def run(self, query, params=None, **kw):
            result = await self._base_run(query, params, **kw)
            if _MUTATION_RE.search(query or ""):
                store.save(self)
            return result

    session = _PersistentSession()
    store.load(session)
    db.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(session))
    db.Neo4jConnector.close_driver = classmethod(lambda cls: None)
    return True
