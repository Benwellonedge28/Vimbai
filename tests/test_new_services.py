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


class TestPrivacyAdminDashboardService:
    def setup_method(self):
        self.client = TestClient(load_service_app("privacy-admin-dashboard-service"))
        use_fake_store("privacy_admin_store", "privacy-admin-dashboard-service")
        self.client.headers.update({"X-User-Id": "root-user", "X-Book-ID": "root-book"})
        self.client = TestClient(load_service_app("privacy-admin-dashboard-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "privacy-admin-dashboard" in resp.json()["service"]

    def test_create_privacy_request(self):
        resp = self.client.post(
            "/requests",
            params={
                "request_type": "access",
                "subject_name": "John Doe",
                "subject_email": "john@example.com",
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["request_type"] == "access"
        assert data["status"] == "pending"

    def test_invalid_request_type(self):
        resp = self.client.post(
            "/requests",
            params={
                "request_type": "invalid",
                "subject_name": "Test",
                "subject_email": "test@test.com",
            },
        )
        assert resp.status_code == 400

    def test_grant_consent(self):
        resp = self.client.post(
            "/consent",
            params={
                "subject_email": "user@test.com",
                "data_type": "marketing",
                "consent_given": True,
            },
        )
        assert resp.status_code == 200
        assert resp.json()["consent_given"] is True

    def test_dashboard(self):
        resp = self.client.get("/dashboard")
        assert resp.status_code == 200
        assert "total_requests" in resp.json()


class TestZeroTrustDataService:
    def setup_method(self):
        # Patch the fake Neo4j driver AFTER app load (package alias registers on
        # main.py exec) and stamp caller identity on every request.
        import importlib.util
        import os as _os

        fake_path = _os.path.join(REPO_ROOT, "zero-trust-data-service", "fake_neo4j.py")
        spec = importlib.util.spec_from_file_location("ztd_root_fake", fake_path)
        fake = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fake)
        shared = fake.FakeSession()
        # the app must load before its package alias exists, so patch afterwards
        self.client = TestClient(load_service_app("zero-trust-data-service"))
        from zero_trust_data_service.database import Neo4jConnector

        Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(shared))
        self.client = TestClient(load_service_app("zero-trust-data-service"))
        self.client.headers.update({"X-User-Id": "root-user", "X-Book-ID": "root-book"})

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "zero-trust-data" in resp.json()["service"]

    def test_create_policy(self):
        resp = self.client.post(
            "/policies",
            json={
                "resource": "/api/financials",
                "required_roles": ["admin", "cfo"],
                "required_clearance": "confidential",
                "mfa_required": True,
                "ip_whitelist": [],
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["resource"] == "/api/financials"

    def test_evaluate_access_granted(self):
        self.client.post(
            "/policies",
            json={
                "resource": "/api/reports",
                "required_roles": ["analyst"],
                "required_clearance": "internal",
                "mfa_required": True,
                "ip_whitelist": [],
            },
        )
        resp = self.client.post(
            "/evaluate",
            json={
                "user_id": "user1",
                "resource": "/api/reports",
                "user_roles": ["analyst"],
                "user_clearance": "internal",
                "mfa_verified": True,
                "source_ip": "192.168.1.1",
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["granted"] is True

    def test_evaluate_access_denied_no_mfa(self):
        self.client.post(
            "/policies",
            json={
                "resource": "/api/secure",
                "required_roles": ["admin"],
                "required_clearance": "restricted",
                "mfa_required": True,
                "ip_whitelist": [],
            },
        )
        resp = self.client.post(
            "/evaluate",
            json={
                "user_id": "user2",
                "resource": "/api/secure",
                "user_roles": ["admin"],
                "user_clearance": "restricted",
                "mfa_verified": False,
                "source_ip": "",
            },
        )
        assert resp.json()["granted"] is False


class TestEncryptedBackupService:
    def setup_method(self):
        self.client = TestClient(load_service_app("encrypted-backup-service"))
        use_fake_store("encrypted_backup_store", "encrypted-backup-service")
        self.client.headers.update({"X-User-Id": "root-user", "X-Book-ID": "root-book"})
        self.client = TestClient(load_service_app("encrypted-backup-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "encrypted-backup" in resp.json()["service"]

    def test_create_backup(self):
        resp = self.client.post(
            "/backup",
            json={
                "service_name": "tax-calculation-service",
                "backup_type": "full",
                "encryption_key_id": "key-001",
                "created_by": "admin",
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["status"] == "completed"
        assert ".enc" in data["file_path"]

    def test_list_backups(self):
        self.client.post(
            "/backup",
            json={"service_name": "test-svc", "backup_type": "incremental"},
        )
        resp = self.client.get("/backups")
        assert resp.status_code == 200
        assert len(resp.json()) > 0


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


class TestDataWarehouseService:
    def setup_method(self):
        # Load first: main.py self-bootstraps the data_warehouse_service package.
        app = load_service_app("data-warehouse-service")
        pkg = sys.modules["data_warehouse_service"]
        fake_path = os.path.join(REPO_ROOT, "data-warehouse-service", "fake_neo4j.py")
        spec = importlib.util.spec_from_file_location("dw_root_fake", fake_path)
        fake = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fake)
        import data_warehouse_service.database as db

        shared = fake.FakeSession()
        db.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(shared))
        self.client = TestClient(app)
        self.client.headers.update({"X-User-Id": "root-user", "X-Book-ID": "root-book"})

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "data-warehouse" in resp.json()["service"]

    def test_create_dimension(self):
        dim_resp = self.client.post(
            "/dimensions",
            params={"name": "dim_date"},
            json=[{"name": "date_id", "type": "int"}, {"name": "year", "type": "int"}],
        )
        assert dim_resp.status_code == 200

    def test_dimension_is_caller_scoped(self):
        self.client.post(
            "/dimensions",
            params={"name": "dim_org"},
            json=[{"name": "org_id", "type": "int"}],
        )
        other = TestClient(self.client.app)
        other.headers.update({"X-User-Id": "other-user", "X-Book-ID": "root-book"})
        assert other.get("/dimensions").json() == []


class TestETLService:
    def setup_method(self):
        self.client = TestClient(load_service_app("etl-service"))
        use_fake_store("etl_store", "etl-service")
        self.client.headers.update({"X-User-Id": "root-user", "X-Book-ID": "root-book"})
        self.client = TestClient(load_service_app("etl-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "etl" in resp.json()["service"]

    def test_create_and_run_pipeline(self):
        resp = self.client.post(
            "/pipelines",
            json={
                "name": "Revenue ETL",
                "description": "Extract revenue data",
                "steps": [
                    {"name": "Extract", "step_type": "extract", "source": "crm_db"},
                    {"name": "Transform", "step_type": "transform"},
                    {"name": "Load", "step_type": "load", "target": "warehouse"},
                ],
            },
        )
        pipeline_id = resp.json()["id"]
        assert resp.status_code == 200

        run_resp = self.client.post(f"/pipelines/{pipeline_id}/run")
        assert run_resp.status_code == 200
        assert run_resp.json()["status"] == "completed"


class TestSOXComplianceService:
    def setup_method(self):
        # Load first: main.py self-bootstraps the sox_compliance_service package.
        app = load_service_app("sox-compliance-service")
        pkg = sys.modules["sox_compliance_service"]
        fake_path = os.path.join(REPO_ROOT, "sox-compliance-service", "fake_neo4j.py")
        spec = importlib.util.spec_from_file_location("sox_root_fake", fake_path)
        fake = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fake)
        import sox_compliance_service.database as db

        shared = fake.FakeSession()
        db.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(shared))
        self.client = TestClient(app)
        self.client.headers.update({"X-User-Id": "root-user", "X-Book-ID": "root-book"})

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "sox-compliance" in resp.json()["service"]

    def test_create_control_and_test(self):
        ctrl_resp = self.client.post(
            "/controls",
            params={
                "control_id_ref": "SOX-ITGC-001",
                "description": "Access control review",
                "control_type": "preventive",
                "control_nature": "automated",
                "frequency": "quarterly",
                "owner": "CISO",
                "process": "IT General Controls",
            },
        )
        assert ctrl_resp.status_code == 200
        ctrl_id = ctrl_resp.json()["id"]

        test_resp = self.client.post(
            f"/controls/{ctrl_id}/test",
            params={
                "test_period": "2026-Q1",
                "tester": "Audit Team",
                "sample_size": 25,
                "exceptions_found": 0,
            },
        )
        assert test_resp.json()["result"] == "pass"

        # caller-scoped: another caller sees no controls
        other = TestClient(self.client.app)
        other.headers.update({"X-User-Id": "other-user", "X-Book-ID": "root-book"})
        assert other.get("/controls").json() == []


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


class TestBenefitsAdminService:
    def setup_method(self):
        # Load first: main.py self-bootstraps the benefits_admin_service package.
        app = load_service_app("benefits-admin-service")
        pkg = sys.modules["benefits_admin_service"]
        if not hasattr(pkg, "_ci_fake_session"):
            fake_path = os.path.join(REPO_ROOT, "benefits-admin-service", "fake_neo4j.py")
            spec = importlib.util.spec_from_file_location("benefits_fake_neo4j", fake_path)
            fake_mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(fake_mod)
            sys.modules["benefits_fake_neo4j"] = fake_mod
            pkg._ci_fake_session = fake_mod.FakeSession()
            import benefits_admin_service.database as db

            db.Neo4jConnector.get_driver = classmethod(lambda cls: fake_mod.FakeDriver(pkg._ci_fake_session))
        self.client = TestClient(app)
        self.headers = {"X-User-Id": "ci-user"}

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "benefits-admin" in resp.json()["service"]

    def test_plan_enroll_and_leave(self):
        plan = self.client.post(
            "/plans",
            params={
                "name": "Pension Plan A",
                "plan_type": "pension",
                "employer_contribution_pct": 5.0,
                "employee_contribution_pct": 3.0,
            },
            headers=self.headers,
        )
        assert plan.status_code == 200, plan.text
        plan_id = plan.json()["id"]

        enr = self.client.post("/enroll", params={"employee_id": "emp001", "plan_id": plan_id}, headers=self.headers)
        assert enr.status_code == 200

        leave = self.client.post(
            "/leave/accrue",
            params={
                "employee_id": "emp001",
                "leave_type": "annual",
                "period": "2026-09",
                "accrued_days": 2.0,
                "taken_days": 0.5,
            },
            headers=self.headers,
        )
        data = leave.json()
        assert leave.status_code == 200
        assert data["balance_days"] == 1.5


