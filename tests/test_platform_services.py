"""
Integration tests for Policy Engine, Webhook, WebSocket, State Machine,
and other platform services.
"""

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service


@pytest.fixture
def policy_client():
    app = load_service("policy-engine-service").main.app
    return TestClient(app)


@pytest.fixture
def webhook_client():
    app = load_service("webhook-service").main.app
    return TestClient(app)


@pytest.fixture
def state_machine_client():
    app = load_service("financial-state-machine-service").main.app
    return TestClient(app)


@pytest.fixture
def integrity_client():
    app = load_service("financial-integrity-service").main.app
    return TestClient(app)


@pytest.fixture
def identity_client():
    app = load_service("financial-identity-service").main.app
    return TestClient(app)


@pytest.fixture
def appropriation_client():
    app = load_service("appropriation-control-service").main.app
    return TestClient(app)


@pytest.fixture
def scenario_client():
    app = load_service("scenario-analysis-service").main.app
    return TestClient(app)


@pytest.fixture
def cash_opt_client():
    app = load_service("cash-optimization-service").main.app
    return TestClient(app)


@pytest.fixture
def sensitivity_client():
    app = load_service("sensitivity-analysis-service").main.app
    return TestClient(app)


@pytest.fixture
def zbb_client():
    app = load_service("zero-based-budgeting-service").main.app
    return TestClient(app)


class TestPolicyEngine:
    def test_health(self, policy_client):
        assert policy_client.get("/").status_code == 200

    def test_create_rule(self, policy_client):
        resp = policy_client.post(
            "/rules/comp-1",
            json={
                "name": "Large Transaction Check",
                "resource_type": "transaction",
                "condition_field": "amount",
                "condition_operator": ">",
                "condition_value": 50000,
                "action": "require_approval",
                "message": "Transaction requires approval",
            },
        )
        assert resp.status_code == 200
        assert resp.json()["action"] == "require_approval"

    def test_evaluate_triggered(self, policy_client):
        policy_client.post(
            "/rules/comp-eval",
            json={
                "name": "Max Amount",
                "resource_type": "transaction",
                "condition_field": "amount",
                "condition_operator": ">",
                "condition_value": 10000,
                "action": "deny",
                "message": "Too large",
            },
        )
        resp = policy_client.post("/evaluate/comp-eval?resource_type=transaction", json={"amount": 50000})
        assert resp.status_code == 200
        data = resp.json()
        assert data["triggered_count"] >= 1
        assert data["blocked"] == True
        assert data["allowed"] == False

    def test_evaluate_not_triggered(self, policy_client):
        policy_client.post(
            "/rules/comp-ok",
            json={
                "name": "Max Amount",
                "resource_type": "transaction",
                "condition_field": "amount",
                "condition_operator": ">",
                "condition_value": 100000,
                "action": "deny",
                "message": "Too large",
            },
        )
        resp = policy_client.post("/evaluate/comp-ok?resource_type=transaction", json={"amount": 5000})
        assert resp.json()["allowed"] == True


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


class TestStateMachine:
    def test_health(self, state_machine_client):
        assert state_machine_client.get("/").status_code == 200

    def test_get_states(self, state_machine_client):
        resp = state_machine_client.get("/states")
        assert resp.status_code == 200
        data = resp.json()
        assert "draft" in data["states"]
        assert "posted" in data["states"]

    def test_create_and_transition(self, state_machine_client):
        create = state_machine_client.post(
            "/documents", json={"company_id": "comp-1", "document_type": "invoice", "reference": "INV-001"}
        )
        assert create.status_code == 200
        doc_id = create.json()["id"]
        assert create.json()["current_state"] == "draft"

        # Valid transition: draft -> pending_approval
        t1 = state_machine_client.post(f"/documents/{doc_id}/transition?to_state=pending_approval&user_id=user1")
        assert t1.status_code == 200
        assert t1.json()["current_state"] == "pending_approval"

        # Valid transition: pending_approval -> approved
        t2 = state_machine_client.post(f"/documents/{doc_id}/transition?to_state=approved&user_id=user1")
        assert t2.status_code == 200
        assert t2.json()["current_state"] == "approved"

        # Valid transition: approved -> posted
        t3 = state_machine_client.post(f"/documents/{doc_id}/transition?to_state=posted")
        assert t3.json()["current_state"] == "posted"

    def test_invalid_transition(self, state_machine_client):
        create = state_machine_client.post("/documents", json={"company_id": "comp-2", "document_type": "invoice"})
        doc_id = create.json()["id"]
        # Invalid: draft -> posted (not allowed)
        resp = state_machine_client.post(f"/documents/{doc_id}/transition?to_state=posted")
        assert resp.status_code == 400

    def test_history(self, state_machine_client):
        create = state_machine_client.post("/documents", json={"company_id": "comp-h", "document_type": "payment"})
        doc_id = create.json()["id"]
        state_machine_client.post(f"/documents/{doc_id}/transition?to_state=pending_approval")
        state_machine_client.post(f"/documents/{doc_id}/transition?to_state=approved")
        resp = state_machine_client.get(f"/documents/{doc_id}/history")
        assert resp.status_code == 200
        data = resp.json()
        assert len(data["history"]) == 2
        assert data["current_state"] == "approved"


class TestFinancialIntegrity:
    def test_health(self, integrity_client):
        assert integrity_client.get("/").status_code == 200

    def test_balance_check_pass(self, integrity_client):
        resp = integrity_client.post("/check/balance?company_id=comp-1&account_id=acc-1&debits=10000&credits=10000")
        assert resp.status_code == 200
        assert resp.json()["passed"] == True

    def test_balance_check_fail(self, integrity_client):
        resp = integrity_client.post("/check/balance?company_id=comp-1&account_id=acc-1&debits=10000&credits=9000")
        assert resp.json()["passed"] == False

    def test_completeness_check(self, integrity_client):
        resp = integrity_client.post(
            "/check/completeness?company_id=comp-1&entity_type=transactions&expected_count=100&actual_count=98"
        )
        assert resp.json()["passed"] == False
        assert resp.json()["missing"] == 2

    def test_integrity_report(self, integrity_client):
        integrity_client.post("/check/balance?company_id=comp-rpt&account_id=a1&debits=100&credits=100")
        resp = integrity_client.get("/report/comp-rpt")
        assert resp.status_code == 200
        assert resp.json()["total_checks"] >= 1


class TestFinancialIdentity:
    def test_health(self, identity_client):
        assert identity_client.get("/").status_code == 200

    def test_create_profile(self, identity_client):
        resp = identity_client.post(
            "/profiles",
            json={"user_id": "user-1", "legal_name": "John Doe", "national_id": "ID123", "email": "john@example.com"},
        )
        assert resp.status_code == 200
        assert resp.json()["verification_status"] == "pending"

    def test_verify_profile(self, identity_client):
        create = identity_client.post("/profiles", json={"user_id": "user-2", "legal_name": "Jane"})
        profile_id = create.json()["id"]
        resp = identity_client.put(f'/profiles/{profile_id}/verify?documents=["passport","utility_bill"]')
        # The documents param is a list, let me try differently
        # Actually it takes a List[str] body
        resp = identity_client.put(f"/profiles/{profile_id}/verify", json=["passport", "utility_bill"])
        assert resp.status_code == 200
        assert resp.json()["status"] == "verified"


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


class TestCashOptimization:
    def test_health(self, cash_opt_client):
        assert cash_opt_client.get("/").status_code == 200

    def test_optimize(self, cash_opt_client):
        cash_opt_client.post(
            "/accounts",
            json={
                "company_id": "comp-1",
                "account_name": "Operating",
                "account_type": "operating",
                "balance": 200000,
                "min_required": 50000,
            },
        )
        cash_opt_client.post(
            "/accounts",
            json={
                "company_id": "comp-1",
                "account_name": "Investment",
                "account_type": "investment",
                "balance": 50000,
                "interest_rate": 0.05,
            },
        )
        resp = cash_opt_client.post("/optimize/comp-1")
        assert resp.status_code == 200
        data = resp.json()
        assert data["total_count"] >= 1
        assert data["potential_annual_benefit"] > 0


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
