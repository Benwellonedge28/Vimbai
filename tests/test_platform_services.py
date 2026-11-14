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


@pytest.fixture
def webhook_client():
    _patch_fake("webhook_service", "webhook_root_fake")
    app = load_service("webhook-service").main.app
    client = TestClient(app)
    client.headers.update(_H)
    return client


@pytest.fixture
def appropriation_client():
    _patch_fake("appropriation_control_service", "appropriation_root_fake")
    app = load_service("appropriation-control-service").main.app
    client = TestClient(app)
    client.headers.update(_H)
    return client


@pytest.fixture
def scenario_client():
    _patch_fake("scenario_analysis_service", "scenario_analysis_root_fake")
    app = load_service("scenario-analysis-service").main.app
    client = TestClient(app)
    client.headers.update(_H)
    return client


@pytest.fixture
def sensitivity_client():
    _patch_fake("sensitivity_analysis_service", "sensitivity_analysis_root_fake")
    app = load_service("sensitivity-analysis-service").main.app
    client = TestClient(app)
    client.headers.update(_H)
    return client


@pytest.fixture
def zbb_client():
    _patch_fake("zero_based_budgeting_service", "zbb_root_fake")
    app = load_service("zero-based-budgeting-service").main.app
    client = TestClient(app)
    client.headers.update(_H)
    return client


# TestPolicyEngine moved to rust/crates/platform/access-control/abac: the service is now the vimbai_abac
# crate, with the decision shape and deny-overrides pinned in Rust tests.


class TestWebhook:
    def test_health(self, webhook_client):
        assert webhook_client.get("/").status_code == 200

    def test_create_endpoint(self, webhook_client):
        resp = webhook_client.post(
            "/endpoints",
            json={
                "company_id": "comp-1",
                "url": "https://example.com/webhook",
                "events": ["invoice.created", "payment.received"],
            },
        )
        assert resp.status_code == 200
        assert "id" in resp.json()

    def test_get_endpoints(self, webhook_client):
        webhook_client.post("/endpoints", json={"company_id": "comp-2", "url": "https://test.com/hook"})
        resp = webhook_client.get("/endpoints/comp-2")
        assert resp.status_code == 200
        assert resp.json()["total"] >= 1


class TestAppropriationControl:
    def test_health(self, appropriation_client):
        assert appropriation_client.get("/").status_code == 200

    def test_create_and_spend(self, appropriation_client):
        create = appropriation_client.post(
            "/appropriations",
            json={"company_id": "comp-1", "department": "IT", "fiscal_year": "2026", "approved_amount": 100000},
        )
        assert create.status_code == 200
        appr_id = create.json()["id"]
        assert create.json()["available_amount"] == 100000

        # Commit some funds
        spend = appropriation_client.post(
            "/transactions", json={"appropriation_id": appr_id, "type": "commit", "amount": 30000}
        )
        assert spend.json()["available"] == 70000  # 100000 - 30000 committed

        # Actually spend
        spend2 = appropriation_client.post(
            "/transactions", json={"appropriation_id": appr_id, "type": "spend", "amount": 30000}
        )
        assert spend2.json()["available"] == 70000  # committed reduced, spent increased

        # Check availability
        check = appropriation_client.get(f"/check/{appr_id}?amount=50000")
        assert check.json()["allowed"] == True


class TestScenarioAnalysis:
    def test_health(self, scenario_client):
        assert scenario_client.get("/").status_code == 200

    def test_create_scenarios(self, scenario_client):
        for stype in ["optimistic", "base", "pessimistic"]:
            scenario_client.post(
                "/scenarios",
                json={
                    "company_id": "comp-1",
                    "name": f"{stype} case",
                    "scenario_type": stype,
                    "projected_revenue": {"optimistic": 200000, "base": 150000, "pessimistic": 100000}[stype],
                    "projected_expenses": 100000,
                },
            )
        resp = scenario_client.get("/scenarios/comp-1")
        assert resp.json()["total"] == 3

    def test_compare(self, scenario_client):
        for rev, name in [(200000, "Best"), (100000, "Worst")]:
            scenario_client.post(
                "/scenarios",
                json={
                    "company_id": "comp-cmp",
                    "name": name,
                    "scenario_type": "custom",
                    "projected_revenue": rev,
                    "projected_expenses": 80000,
                },
            )
        resp = scenario_client.get("/compare/comp-cmp")
        assert resp.status_code == 200
        assert resp.json()["best_case"] == "Best"
        assert resp.json()["worst_case"] == "Worst"


class TestSensitivityAnalysis:
    def test_health(self, sensitivity_client):
        assert sensitivity_client.get("/").status_code == 200

    def test_analyze(self, sensitivity_client):
        resp = sensitivity_client.post(
            "/analyze",
            json={
                "company_id": "comp-1",
                "target_metric": "net_profit",
                "base_target_value": 100000,
                "variables": [
                    {"name": "revenue", "base_value": 500000, "change_pct": 10},
                    {"name": "costs", "base_value": 400000, "change_pct": 10},
                ],
                "change_steps": [-10, 0, 10],
            },
        )
        assert resp.status_code == 200
        data = resp.json()
        assert len(data["results"]) == 6  # 2 variables * 3 steps
        assert data["most_sensitive_variable"] != ""


class TestZeroBasedBudgeting:
    def test_health(self, zbb_client):
        assert zbb_client.get("/").status_code == 200

    def test_create_package_with_items(self, zbb_client):
        resp = zbb_client.post(
            "/packages",
            json={
                "company_id": "comp-1",
                "period": "2026-Q1",
                "name": "IT Budget",
                "department": "IT",
                "items": [
                    {
                        "department": "IT",
                        "category": "software",
                        "description": "Licenses",
                        "amount": 50000,
                        "justification": "Required for ops",
                        "priority": 1,
                    },
                    {
                        "department": "IT",
                        "category": "hardware",
                        "description": "Servers",
                        "amount": 30000,
                        "justification": "Aging equipment",
                        "priority": 2,
                    },
                ],
            },
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_amount"] == 80000
        assert data["status"] == "draft"

    def test_zbb_summary(self, zbb_client):
        zbb_client.post(
            "/packages",
            json={
                "company_id": "comp-sum",
                "name": "Finance Q1",
                "department": "Finance",
                "period": "2026-Q1",
                "items": [
                    {
                        "department": "Finance",
                        "category": "travel",
                        "description": "Audit travel",
                        "amount": 10000,
                        "justification": "Need",
                    }
                ],
            },
        )
        resp = zbb_client.get("/summary/comp-sum")
        assert resp.status_code == 200
        assert resp.json()["total_packages"] >= 1
