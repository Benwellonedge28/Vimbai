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
    """Dynamically load a service's FastAPI app from a hyphenated directory name."""
    main_path = os.path.join(REPO_ROOT, service_dir, "main.py")
    spec = importlib.util.spec_from_file_location(f"{service_dir}.main", main_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.app


class TestBankingIntegrationService:
    def setup_method(self):
        self.client = TestClient(load_service_app("banking-integration-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "banking-integration" in resp.json()["service"]

    def test_create_bank_connection(self):
        resp = self.client.post(
            "/connect",
            params={
                "bank_name": "Standard Bank",
                "account_number": "1234567890",
                "api_key": "test-key",
                "account_type": "checking",
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["bank_name"] == "Standard Bank"
        assert data["status"] == "active"

    def test_sync_transactions(self):
        conn = self.client.post(
            "/connect",
            params={
                "bank_name": "CBZ",
                "account_number": "9876543210",
                "api_key": "test-key",
                "account_type": "savings",
            },
        )
        conn_id = conn.json()["id"]
        resp = self.client.post(f"/sync/{conn_id}")
        assert resp.status_code == 200
        assert resp.json()["status"] == "success"


class TestPrivacyAdminDashboardService:
    def setup_method(self):
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
        self.client = TestClient(load_service_app("zero-trust-data-service"))

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


class TestFamilyCommunityGroupService:
    def setup_method(self):
        self.client = TestClient(load_service_app("family-community-group-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "family-community-group" in resp.json()["service"]

    def test_create_group(self):
        resp = self.client.post(
            "/groups",
            params={
                "name": "Mukando Group A",
                "description": "Monthly savings group",
                "contribution_frequency": "monthly",
                "contribution_amount": 100.0,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["name"] == "Mukando Group A"
        assert data["contribution_amount"] == 100.0

    def test_add_member_and_contribute(self):
        group_resp = self.client.post(
            "/groups",
            params={
                "name": "Test Group",
                "contribution_frequency": "weekly",
                "contribution_amount": 50.0,
            },
        )
        group_id = group_resp.json()["id"]

        member_resp = self.client.post(
            f"/groups/{group_id}/members",
            params={
                "name": "Alice",
                "email": "alice@test.com",
                "contribution_amount": 50.0,
            },
        )
        member_id = member_resp.json()["id"]

        contrib_resp = self.client.post(
            f"/groups/{group_id}/contribute",
            params={"member_id": member_id, "amount": 50.0},
        )
        assert contrib_resp.status_code == 200
        assert contrib_resp.json()["amount"] == 50.0


class TestOrgAuthorizationEngine:
    def setup_method(self):
        self.client = TestClient(load_service_app("org-authorization-engine"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "org-authorization" in resp.json()["service"]

    def test_create_role_and_check(self):
        role_resp = self.client.post(
            "/roles",
            params={
                "name": "finance_admin",
                "description": "Finance administrator",
                "permissions": ["read:reports", "write:reports"],
            },
        )
        role_id = role_resp.json()["id"]

        assign_resp = self.client.post(
            "/assign",
            params={"user_id": "user1", "org_id": "org1", "role_id": role_id},
        )
        assert assign_resp.status_code == 200

        check_resp = self.client.post(
            "/check",
            json={
                "user_id": "user1",
                "org_id": "org1",
                "permission": "read:reports",
                "resource": "",
            },
        )
        assert check_resp.json()["allowed"] is True


class TestDistributedWorkflowOrchestrationService:
    def setup_method(self):
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


class TestReportDistributionService:
    def setup_method(self):
        self.client = TestClient(load_service_app("report-distribution-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "report-distribution" in resp.json()["service"]

    def test_create_list_and_subscribe(self):
        list_resp = self.client.post(
            "/lists",
            params={
                "name": "Finance Team",
                "recipients": ["cfo@vimbai.com", "ceo@vimbai.com"],
            },
        )
        list_id = list_resp.json()["id"]

        sub_resp = self.client.post(
            "/subscriptions",
            params={
                "report_type": "monthly_summary",
                "distribution_list_id": list_id,
                "frequency": "monthly",
            },
        )
        assert sub_resp.status_code == 200

        sub_id = sub_resp.json()["id"]
        dist_resp = self.client.post(f"/subscriptions/{sub_id}/distribute")
        assert dist_resp.status_code == 200
        assert dist_resp.json()["status"] == "sent"


class TestDataWarehouseService:
    def setup_method(self):
        self.client = TestClient(load_service_app("data-warehouse-service"))

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


class TestETLService:
    def setup_method(self):
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


class TestInvestmentMonitoringService:
    def setup_method(self):
        self.client = TestClient(load_service_app("investment-monitoring-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "investment-monitoring" in resp.json()["service"]

    def test_portfolio_and_performance(self):
        port_resp = self.client.post(
            "/portfolios",
            params={
                "name": "Growth Portfolio",
                "target_return": 12.0,
                "risk_tolerance": "moderate",
            },
        )
        port_id = port_resp.json()["id"]

        hold_resp = self.client.post(
            f"/portfolios/{port_id}/holdings",
            params={
                "instrument_name": "AAPL",
                "instrument_type": "equity",
                "quantity": 100,
                "purchase_price": 150.0,
                "current_price": 175.0,
            },
        )
        assert hold_resp.status_code == 200

        perf_resp = self.client.post(f"/portfolios/{port_id}/performance")
        data = perf_resp.json()
        assert perf_resp.status_code == 200
        assert data["total_value"] == 17500.0
        assert data["unrealized_gain"] == 2500.0


class TestIntangibleAssetsService:
    def setup_method(self):
        self.client = TestClient(load_service_app("intangible-assets-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "intangible-assets" in resp.json()["service"]

    def test_create_and_amortize(self):
        resp = self.client.post(
            "/assets",
            params={
                "name": "Software License",
                "asset_type": "software",
                "cost": 120000.0,
                "useful_life_years": 3,
                "acquisition_date": "2026-01-01T00:00:00",
                "residual_value": 0.0,
                "amortization_method": "straight_line",
            },
        )
        asset_id = resp.json()["id"]
        assert resp.status_code == 200

        amort_resp = self.client.post(f"/assets/{asset_id}/amortize", params={"period": "2026-01"})
        data = amort_resp.json()
        assert amort_resp.status_code == 200
        assert data["amortization_amount"] > 0


class TestSOXComplianceService:
    def setup_method(self):
        self.client = TestClient(load_service_app("sox-compliance-service"))

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
        ctrl_id = ctrl_resp.json()["id"]
        assert ctrl_resp.status_code == 200

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


class TestTreasuryPolicyService:
    def setup_method(self):
        self.client = TestClient(load_service_app("treasury-policy-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "treasury-policy" in resp.json()["service"]

    def test_create_policy_and_check_limit(self):
        pol_resp = self.client.post(
            "/policies",
            params={
                "name": "FX Exposure Policy",
                "description": "Limit FX exposure",
                "policy_category": "fx_risk",
                "effective_date": "2026-01-01T00:00:00",
                "approved_by": "CFO",
            },
        )
        pol_id = pol_resp.json()["id"]

        lim_resp = self.client.post(
            f"/policies/{pol_id}/limits",
            params={
                "limit_type": "exposure",
                "limit_value": 1000000.0,
                "currency": "USD",
                "warning_threshold": 0.8,
            },
        )
        lim_id = lim_resp.json()["id"]

        check_resp = self.client.post(f"/limits/{lim_id}/check", params={"checked_value": 750000.0})
        assert check_resp.json()["compliant"] is True

        breach_resp = self.client.post(f"/limits/{lim_id}/check", params={"checked_value": 1100000.0})
        assert breach_resp.json()["compliant"] is False


class TestCashManagementService:
    def setup_method(self):
        self.client = TestClient(load_service_app("cash-management-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "cash-management" in resp.json()["service"]

    def test_create_accounts_and_transfer(self):
        acct1 = self.client.post(
            "/accounts",
            params={
                "account_name": "Operating",
                "bank": "Standard Bank",
                "account_number": "1111111",
                "balance": 100000.0,
                "type": "operating",
            },
        )
        acct2 = self.client.post(
            "/accounts",
            params={
                "account_name": "Reserve",
                "bank": "CBZ",
                "account_number": "2222222",
                "balance": 50000.0,
                "type": "reserve",
            },
        )

        transfer_resp = self.client.post(
            "/transfers",
            params={
                "from_account_id": acct1.json()["id"],
                "to_account_id": acct2.json()["id"],
                "amount": 20000.0,
            },
        )
        assert transfer_resp.status_code == 200
        assert transfer_resp.json()["status"] == "completed"

    def test_liquidity(self):
        self.client.post(
            "/accounts",
            params={
                "account_name": "Test Operating",
                "bank": "Test Bank",
                "account_number": "9999999",
                "balance": 50000.0,
                "type": "operating",
            },
        )
        resp = self.client.post("/liquidity", params={"short_term_obligations": 30000.0})
        data = resp.json()
        assert data["liquidity_ratio"] > 0


class TestIFRSReportingService:
    def setup_method(self):
        self.client = TestClient(load_service_app("ifrs-reporting-service"))

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
        )
        assert note_resp.status_code == 200
        assert note_resp.json()["title"] == "Accounting Policies"


class TestFixedAssetsRegisterService:
    def setup_method(self):
        self.client = TestClient(load_service_app("fixed-assets-register-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "fixed-assets-register" in resp.json()["service"]

    def test_register_and_depreciate(self):
        resp = self.client.post(
            "/assets",
            params={
                "asset_code": "FA-001",
                "asset_name": "Delivery Truck",
                "category": "vehicles",
                "acquisition_date": "2026-01-01T00:00:00",
                "acquisition_cost": 50000.0,
                "useful_life_years": 5,
                "salvage_value": 5000.0,
                "depreciation_method": "straight_line",
            },
        )
        asset_id = resp.json()["id"]
        assert resp.status_code == 200

        dep_resp = self.client.post(f"/assets/{asset_id}/depreciate", params={"period": "2026-01"})
        data = dep_resp.json()
        assert dep_resp.status_code == 200
        assert data["depreciation_amount"] > 0

        summary = self.client.get("/summary")
        assert summary.status_code == 200
        assert "total_assets" in summary.json()


class TestLifecycleCostingService:
    def setup_method(self):
        self.client = TestClient(load_service_app("lifecycle-costing-service"))

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
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_lifecycle_cost"] == 105000.0
        assert data["annual_equivalent_cost"] > 0


class TestRollingForecastService:
    def setup_method(self):
        self.client = TestClient(load_service_app("rolling-forecast-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "rolling-forecast" in resp.json()["service"]

    def test_create_and_update_forecast(self):
        resp = self.client.post(
            "/forecasts",
            params={
                "name": "Revenue Forecast",
                "metric": "revenue",
                "frequency": "monthly",
                "horizon_periods": 3,
            },
            json=[
                {"period": "2026-01", "forecast_value": 100000},
                {"period": "2026-02", "forecast_value": 110000},
                {"period": "2026-03", "forecast_value": 120000},
            ],
        )
        forecast_id = resp.json()["id"]
        assert resp.status_code == 200

        actual_resp = self.client.post(
            f"/forecasts/{forecast_id}/update-actual",
            params={"period": "2026-01", "actual_value": 95000},
        )
        data = actual_resp.json()
        assert actual_resp.status_code == 200
        assert "variance" in data


class TestAuthorizedShareCapitalService:
    def setup_method(self):
        # Load first: main.py self-bootstraps the authorized_share_capital_service package.
        app = load_service_app("authorized-share-capital-service")
        pkg = sys.modules["authorized_share_capital_service"]
        if not hasattr(pkg, "_ci_fake_session"):
            fake_path = os.path.join(REPO_ROOT, "authorized-share-capital-service", "fake_neo4j.py")
            spec = importlib.util.spec_from_file_location("asc_fake_neo4j_ci", fake_path)
            fake_mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(fake_mod)
            sys.modules["asc_fake_neo4j_ci"] = fake_mod
            pkg._ci_fake_session = fake_mod.FakeSession()
            import authorized_share_capital_service.database as db

            db.Neo4jConnector.get_driver = classmethod(lambda cls: fake_mod.FakeDriver(pkg._ci_fake_session))
        self.client = TestClient(app)
        self.headers = {"X-User-Id": "ci-user"}

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "authorized-share-capital" in resp.json()["service"]

    def test_create_class_and_issue(self):
        cls_resp = self.client.post(
            "/share-classes",
            params={
                "name": "Ordinary Shares",
                "authorized_shares": 1000000,
                "par_value": 1.0,
                "voting_rights": "ordinary",
            },
            headers=self.headers,
        )
        cls_id = cls_resp.json()["id"]

        issue_resp = self.client.post(
            f"/share-classes/{cls_id}/issue",
            params={"number_of_shares": 100000, "issue_price": 5.0},
            headers=self.headers,
        )
        assert issue_resp.status_code == 200
        assert issue_resp.json()["total_proceeds"] == 500000.0

        summary = self.client.get("/summary", headers=self.headers)
        assert summary.json()["total_issued"] == 100000


class TestIntercompanyService:
    def setup_method(self):
        self.client = TestClient(load_service_app("intercompany-service"))

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


class TestTreasuryRiskService:
    def setup_method(self):
        self.client = TestClient(load_service_app("treasury-risk-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "treasury-risk" in resp.json()["service"]

    def test_calculate_var(self):
        resp = self.client.post(
            "/var",
            params={
                "portfolio_value": 1000000.0,
                "confidence_level": 0.95,
                "holding_period_days": 1,
                "daily_volatility": 0.01,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["var_amount"] > 0
        assert data["var_pct"] > 0

    def test_stress_test(self):
        scenario = self.client.post(
            "/scenarios",
            params={
                "name": "Market Crash",
                "description": "20% market decline",
                "shock_type": "market_crash",
                "shock_magnitude": 20.0,
            },
        )
        scenario_id = scenario.json()["id"]

        result = self.client.post(f"/scenarios/{scenario_id}/run", params={"portfolio_value": 1000000.0})
        data = result.json()
        assert result.status_code == 200
        assert data["impact"] < 0
        assert data["impact_pct"] == -20.0

    def test_dashboard(self):
        resp = self.client.get("/dashboard")
        assert resp.status_code == 200
        assert "total_exposures" in resp.json()


class TestReportAutomationService:
    def setup_method(self):
        self.client = TestClient(load_service_app("report-automation-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "report-automation" in resp.json()["service"]

    def test_template_schedule_and_run(self):
        tpl_resp = self.client.post(
            "/templates",
            params={
                "name": "Monthly Financial Summary",
                "report_type": "financial_summary",
                "format": "pdf",
            },
        )
        tpl_id = tpl_resp.json()["id"]

        sch_resp = self.client.post(
            "/schedules",
            params={
                "template_id": tpl_id,
                "name": "Monthly Run",
                "cron_expression": "0 0 1 * *",
            },
            json=["cfo@vimbai.com"],
        )
        sch_id = sch_resp.json()["id"]

        run_resp = self.client.post(f"/schedules/{sch_id}/run", params={"period": "2026-09"})
        assert run_resp.status_code == 200
        assert run_resp.json()["status"] == "delivered"


class TestDisposalGroupService:
    def setup_method(self):
        self.client = TestClient(load_service_app("disposal-group-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "disposal-group" in resp.json()["service"]

    def test_create_group_and_impairment(self):
        grp = self.client.post(
            "/groups",
            params={
                "name": "Discontinued Operations",
                "description": "Business segment for sale",
                "disposal_method": "sale",
            },
        )
        grp_id = grp.json()["id"]

        asset = self.client.post(
            f"/groups/{grp_id}/assets",
            params={
                "asset_id": "FA-100",
                "asset_name": "Factory Building",
                "carrying_amount": 500000.0,
                "fair_value": 450000.0,
            },
        )
        assert asset.status_code == 200

        imp = self.client.post(
            f"/groups/{grp_id}/impairment-test",
            params={"fair_value_less_costs": 440000.0},
        )
        data = imp.json()
        assert imp.status_code == 200
        assert data["impairment_amount"] == 60000.0


class TestActivityBasedBudgetService:
    def setup_method(self):
        self.client = TestClient(load_service_app("activity-based-budget-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "activity-based-budget" in resp.json()["service"]

    def test_create_activity_and_budget(self):
        act = self.client.post(
            "/activities",
            params={
                "name": "Quality Inspection",
                "cost_pool": "Manufacturing",
                "driver": "inspections",
                "driver_rate": 50.0,
            },
        )
        act_id = act.json()["id"]

        bgt = self.client.post(
            "/budgets",
            params={"name": "Q1 Budget", "fiscal_year": "2026", "period": "2026-Q1"},
            json=[{"activity_id": act_id, "expected_driver_volume": 100, "notes": "Q1 inspections"}],
        )
        data = bgt.json()
        assert bgt.status_code == 200
        assert data["total_budget"] == 5000.0


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


class TestPerformanceBenchmarkingService:
    def setup_method(self):
        self.client = TestClient(load_service_app("performance-benchmarking-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "performance-benchmarking" in resp.json()["service"]

    def test_metric_and_benchmark(self):
        met = self.client.post(
            "/metrics",
            params={
                "name": "Current Ratio",
                "category": "financial",
                "unit": "ratio",
                "industry_median": 1.5,
                "industry_top_quartile": 2.5,
                "industry_bottom_quartile": 0.8,
            },
        )
        met_id = met.json()["id"]

        bench = self.client.post(
            "/benchmark",
            params={"org_value": 2.0, "metric_id": met_id, "period": "2026"},
        )
        data = bench.json()
        assert bench.status_code == 200
        assert data["rating"] == "above_average"
        assert data["percentile_rank"] > 50


class TestLeaseTerminationService:
    def setup_method(self):
        self.client = TestClient(load_service_app("lease-termination-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "lease-termination" in resp.json()["service"]

    def test_create_and_settle(self):
        term = self.client.post(
            "/terminations",
            params={
                "lease_id": "LEASE-001",
                "termination_date": "2026-06-01T00:00:00",
                "original_end_date": "2027-06-01T00:00:00",
                "remaining_payments": 12,
                "remaining_payment_amount": 5000.0,
                "early_termination_penalty": 10000.0,
            },
        )
        term_id = term.json()["id"]
        assert term.status_code == 200
        assert term.json()["settlement_amount"] == 70000.0

        settle = self.client.post(
            f"/terminations/{term_id}/settlement",
            params={"asset_return_value": 20000.0},
        )
        data = settle.json()
        assert settle.status_code == 200
        assert data["net_settlement"] == 50000.0


class TestTreasuryReportingService:
    def setup_method(self):
        self.client = TestClient(load_service_app("treasury-reporting-service"))

    def test_health(self):
        resp = self.client.get("/health")
        assert resp.status_code == 200
        assert "treasury-reporting" in resp.json()["service"]

    def test_cash_position_report(self):
        resp = self.client.post(
            "/reports/cash-position",
            params={"period": "2026-09", "generated_by": "treasury"},
            json=[
                {
                    "account_id": "1",
                    "account_name": "Operating",
                    "currency": "USD",
                    "balance": 50000,
                    "balance_usd": 50000,
                },
                {
                    "account_id": "2",
                    "account_name": "Reserve",
                    "currency": "USD",
                    "balance": 30000,
                    "balance_usd": 30000,
                },
            ],
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["summary"]["total_cash_usd"] == 80000.0
        assert data["summary"]["total_accounts"] == 2
