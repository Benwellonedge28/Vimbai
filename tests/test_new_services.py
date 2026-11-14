"""
Integration tests for the 31 newly implemented services.
Tests real business logic endpoints across all new services.
"""

import importlib.util
import os
import sys

import pytest
from fastapi.testclient import TestClient

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, REPO_ROOT)


def load_service_app(service_dir):
    from tests.conftest import service_missing

    if service_missing(service_dir):
        import pytest

        pytest.skip(f"{service_dir} has been deleted; its maths lives in the Rust library")
    """Dynamically load a service's FastAPI app from a hyphenated directory name."""
    main_path = os.path.join(REPO_ROOT, service_dir, "main.py")
    spec = importlib.util.spec_from_file_location(f"{service_dir}.main", main_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.app


def use_fake_store(store_module_name, service_dir):
    """Point a converted service's Book-scoped store at the fake Neo4j driver."""
    fake_path = os.path.join(REPO_ROOT, service_dir, "fake_neo4j.py")
    spec = importlib.util.spec_from_file_location(f"{store_module_name}_fake", fake_path)
    fake = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fake)
    store = sys.modules[store_module_name]
    store.set_driver(fake.FakeDriver(fake.FakeSession()))
    return store


class TestDistributedWorkflowOrchestrationService:
    def setup_method(self):
        self.client = TestClient(load_service_app("distributed-workflow-orchestration-service"))
        use_fake_store("workflow_store", "distributed-workflow-orchestration-service")
        self.client.headers.update({"X-User-Id": "root-user", "X-Book-ID": "root-book"})
        self.client = TestClient(load_service_app("distributed-workflow-orchestration-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "distributed-workflow-orchestration" in resp.json()["service"]

    def test_create_and_execute_workflow(self):
        resp = self.client.post(
            "/workflows",
            json={
                "name": "E2E Report Pipeline",
                "description": "Generate and distribute reports",
                "steps": [
                    {
                        "name": "Generate",
                        "service_url": "http://report-svc",
                        "endpoint": "/generate",
                    },
                    {
                        "name": "Distribute",
                        "service_url": "http://dist-svc",
                        "endpoint": "/send",
                    },
                ],
            },
        )
        workflow_id = resp.json()["id"]
        assert resp.status_code == 200

        exec_resp = self.client.post(f"/workflows/{workflow_id}/execute")
        assert exec_resp.status_code == 200
        assert exec_resp.json()["status"] == "completed"


class TestIFRSReportingService:
    """IFRS reporting is Book-scoped since the Neo4j conversion; the fake driver is
    bound the same way as the other converted services."""

    def setup_method(self):
        app = load_service_app("ifrs-reporting-service")

        fake_path = os.path.join(REPO_ROOT, "ifrs-reporting-service", "fake_neo4j.py")
        spec = importlib.util.spec_from_file_location("ifrs_root_fake", fake_path)
        fake = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fake)

        db = importlib.import_module("ifrs_reporting_service.database")
        self._session = fake.FakeSession()
        db.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(self._session))
        self._headers = {"X-User-Id": "user-ifrs", "X-Book-ID": "book-ifrs"}
        self.client = TestClient(app)

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "ifrs-reporting" in resp.json()["service"]

    def test_create_report_and_note(self):
        resp = self.client.post(
            "/reports",
            params={
                "report_type": "balance_sheet",
                "ifrs_standard": "IAS1",
                "period": "2026",
                "reporting_date": "2026-12-31T00:00:00",
            },
            json={"assets": 500000, "liabilities": 200000},
            headers=self._headers,
        )
        report_id = resp.json()["id"]
        assert resp.status_code == 200

        note_resp = self.client.post(
            f"/reports/{report_id}/notes",
            params={
                "note_number": 1,
                "title": "Accounting Policies",
                "ifrs_reference": "IAS1.10",
            },
            headers=self._headers,
        )
        assert note_resp.status_code == 200
        assert note_resp.json()["title"] == "Accounting Policies"


class TestLifecycleCostingService:
    def setup_method(self):
        # Load first: main.py self-bootstraps the lifecycle_costing_service package.
        app = load_service_app("lifecycle-costing-service")
        pkg = sys.modules["lifecycle_costing_service"]
        if not hasattr(pkg, "_ci_fake_session"):
            fake_path = os.path.join(REPO_ROOT, "lifecycle-costing-service", "fake_neo4j.py")
            spec = importlib.util.spec_from_file_location("lc_fake_neo4j_ci", fake_path)
            fake_mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(fake_mod)
            sys.modules["lc_fake_neo4j_ci"] = fake_mod
            pkg._ci_fake_session = fake_mod.FakeSession()
            import lifecycle_costing_service.database as db

            db.Neo4jConnector.get_driver = classmethod(lambda cls: fake_mod.FakeDriver(pkg._ci_fake_session))
        self.client = TestClient(app)
        self.headers = {"X-User-Id": "ci-user", "X-Book-ID": "ci-book"}

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "lifecycle-costing" in resp.json()["service"]

    def test_create_model(self):
        resp = self.client.post(
            "/models",
            params={"asset_name": "Server Infrastructure", "discount_rate": 0.1},
            json=[
                {"phase": "acquisition", "duration_years": 0, "annual_cost": 0, "one_time_cost": 50000},
                {"phase": "operation", "duration_years": 5, "annual_cost": 10000, "one_time_cost": 0},
                {"phase": "disposal", "duration_years": 0, "annual_cost": 0, "one_time_cost": 5000},
            ],
            headers=self.headers,
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_lifecycle_cost"] == 105000.0
        assert data["annual_equivalent_cost"] > 0

    def test_model_is_persisted_for_its_owner_only(self):
        created = self.client.post(
            "/models",
            params={"asset_name": "Server Infrastructure"},
            json=[{"phase": "acquisition", "one_time_cost": 50000}],
            headers=self.headers,
        ).json()

        listed = self.client.get("/models", headers=self.headers).json()
        assert created["id"] in [m["id"] for m in listed]

        stranger = self.client.get("/models", headers={"X-User-Id": "other-user"}).json()
        assert created["id"] not in [m["id"] for m in stranger]


class TestIntercompanyService:
    def setup_method(self):
        # Load first: main.py self-bootstraps the intercompany_service package.
        app = load_service_app("intercompany-service")
        fake_path = os.path.join(REPO_ROOT, "intercompany-service", "fake_neo4j.py")
        spec = importlib.util.spec_from_file_location("ic_root_fake", fake_path)
        fake = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fake)
        import intercompany_service.database as db

        shared = fake.FakeSession()
        db.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(shared))
        self.client = TestClient(app)
        self.client.headers.update({"X-User-Id": "root-user", "X-Book-ID": "root-book"})

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "intercompany" in resp.json()["service"]

    def test_create_entities_and_transaction(self):
        ent1 = self.client.post(
            "/entities",
            params={"name": "Vimbai ZW", "legal_entity_code": "VZ001", "currency": "USD"},
        )
        ent2 = self.client.post(
            "/entities",
            params={"name": "Vimbai ZA", "legal_entity_code": "VZ002", "currency": "USD"},
        )

        txn = self.client.post(
            "/transactions",
            params={
                "from_entity_id": ent1.json()["id"],
                "to_entity_id": ent2.json()["id"],
                "transaction_type": "service_fee",
                "amount": 10000.0,
            },
        )
        assert txn.status_code == 200
        assert txn.json()["amount"] == 10000.0


