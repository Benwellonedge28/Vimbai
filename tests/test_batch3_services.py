"""
Integration tests for batch 3 of upgraded services (tax, costing, trade, reporting).
"""

import importlib
import os
import sys

import pytest
from fastapi.testclient import TestClient

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, REPO_ROOT)


def load_app(service_dir):
    main_path = os.path.join(REPO_ROOT, service_dir, "main.py")
    spec = importlib.util.spec_from_file_location(f"{service_dir}.main", main_path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.app


class TestTaxRiskService:
    def setup_method(self):
        self.client = TestClient(load_app("tax-risk-service"))

    def test_assess(self):
        resp = self.client.post(
            "/assess",
            json={
                "company_id": "comp-1",
                "risks": [
                    {
                        "description": "TP gap",
                        "risk_type": "transfer_pricing",
                        "potential_exposure": 200000,
                        "probability": 0.4,
                    },
                    {"description": "VAT gap", "risk_type": "vat_gap", "potential_exposure": 50000, "probability": 0.3},
                ],
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_exposure"] == 250000
        assert data["weighted_exposure"] == 95000
        assert data["overall_risk_level"] in ("low", "medium", "high", "critical")
        assert len(data["recommendations"]) > 0


class TestTaxComplianceService:
    def setup_method(self):
        self.client = TestClient(load_app("tax-compliance-service"))

    def test_obligation_lifecycle(self):
        obl = self.client.post(
            "/obligations",
            json={
                "company_id": "comp-1",
                "obligation_type": "vat_return",
                "description": "Q1 VAT Return",
                "due_date": "2026-04-30T00:00:00Z",
                "amount": 15000,
                "filing_frequency": "quarterly",
            },
        )
        assert obl.status_code == 200
        obl_id = obl.json()["id"]

        filed = self.client.post(f"/obligations/{obl_id}/file", params={"company_id": "comp-1", "filed_amount": 15000})
        assert filed.json()["filed"] is True

        summary = self.client.get("/summary", params={"company_id": "comp-1"})
        data = summary.json()
        assert data["filed"] >= 1
        assert data["compliance_score"] > 0


class TestTaxProvisionService:
    def setup_method(self):
        self.client = TestClient(load_app("tax-provision-service"))

    def test_provision_calculation(self):
        resp = self.client.post(
            "/calculate",
            json={
                "company_id": "comp-1",
                "fiscal_year": 2026,
                "pre_tax_income": 500000,
                "statutory_rate": 0.25,
                "permanent_differences": 10000,
                "temp_differences": [
                    {"description": "Depreciation", "book_amount": 50000, "tax_amount": 30000, "type": "taxable"},
                    {"description": "Warranty", "book_amount": 10000, "tax_amount": 0, "type": "deductible"},
                ],
                "loss_carryforward": 0,
                "valuation_allowance": 0,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["current_tax_expense"] > 0
        assert data["deferred_tax_liabilities"] > 0
        assert data["deferred_tax_assets"] > 0
        assert data["effective_rate"] < data["statutory_rate"] + 0.05


class TestTaxPlanningService:
    def setup_method(self):
        self.client = TestClient(load_app("tax-planning-service"))

    def test_plan(self):
        resp = self.client.post(
            "/plan",
            json={
                "company_id": "comp-1",
                "fiscal_year": 2026,
                "current_taxable_income": 1000000,
                "current_tax": 250000,
                "strategies": [
                    {
                        "name": "R&D Credit",
                        "description": "Claim R&D credits",
                        "strategy_type": "credit",
                        "estimated_savings": 50000,
                        "implementation_cost": 5000,
                        "risk_level": "low",
                    },
                    {
                        "name": "Accelerated Depreciation",
                        "description": "Use accelerated depreciation",
                        "strategy_type": "timing",
                        "estimated_savings": 30000,
                        "implementation_cost": 2000,
                        "risk_level": "low",
                    },
                ],
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_savings"] == 80000
        assert data["net_benefit"] == 73000
        assert data["projected_tax"] == 170000
        assert len(data["recommended_strategies"]) >= 1


class TestRDTaxService:
    def setup_method(self):
        self.client = TestClient(load_app("r-and-d-tax-service"))

    def test_credit_calculation(self):
        resp = self.client.post(
            "/calculate",
            json={
                "company_id": "comp-1",
                "fiscal_year": 2026,
                "expenditures": [
                    {"category": "wages", "description": "Engineer salaries", "amount": 200000, "qualifies": True},
                    {"category": "supplies", "description": "Lab materials", "amount": 50000, "qualifies": True},
                    {"category": "overhead", "description": "Office rent", "amount": 30000, "qualifies": False},
                ],
                "credit_rate": 0.15,
                "alternative_rate": 0.20,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["qualifying_expenditure"] == 250000
        assert data["non_qualifying"] == 30000
        assert data["credit_regular"] == 37500
        assert data["credit_alternative"] == 50000
        assert "wages" in data["expenditure_breakdown"]


class TestGroupTaxService:
    def setup_method(self):
        self.client = TestClient(load_app("group-tax-service"))

    def test_consolidation(self):
        resp = self.client.post(
            "/consolidate",
            json={
                "group_id": "grp-1",
                "fiscal_year": 2026,
                "subsidiaries": [
                    {
                        "entity_id": "e1",
                        "entity_name": "Sub A",
                        "jurisdiction": "ZW",
                        "pre_tax_income": 300000,
                        "tax_paid": 75000,
                        "tax_rate": 0.25,
                    },
                    {
                        "entity_id": "e2",
                        "entity_name": "Sub B",
                        "jurisdiction": "ZA",
                        "pre_tax_income": -50000,
                        "tax_paid": 0,
                        "tax_rate": 0.28,
                    },
                ],
                "group_tax_rate": 0.25,
                "intercompany_eliminations": 20000,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["consolidated_income"] < 300000  # reduced by eliminations and losses
        assert len(data["subsidiary_summary"]) == 2


class TestTransferPricingService:
    def setup_method(self):
        self.client = TestClient(load_app("transfer-pricing-service"))

    def test_analysis(self):
        resp = self.client.post(
            "/analyze",
            json={
                "company_id": "comp-1",
                "transactions": [
                    {
                        "company_id": "comp-1",
                        "product_service": "Consulting",
                        "selling_entity": "Parent",
                        "buying_entity": "Sub",
                        "transaction_value": 100000,
                        "arm_length_range_min": 90000,
                        "arm_length_range_max": 120000,
                        "method": "comparable_uncontrolled_price",
                    },
                    {
                        "company_id": "comp-1",
                        "product_service": "License",
                        "selling_entity": "Parent",
                        "buying_entity": "Sub",
                        "transaction_value": 50000,
                        "arm_length_range_min": 70000,
                        "arm_length_range_max": 90000,
                        "method": "transactional_net_margin",
                    },
                ],
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["compliant_transactions"] == 1
        assert data["non_compliant_transactions"] == 1
        assert data["total_adjustment_needed"] == 20000  # 70000-50000
        assert len(data["documentation_required"]) >= 3


class TestTradeFinanceService:
    """trade-finance is now Neo4j-backed with X-User-Id auth + Book scoping."""

    H = {"X-User-Id": "upg-tf-user"}

    @pytest.fixture(autouse=True)
    def _patch_fake_db(self):
        # load the app first so the package alias is registered, then patch the driver
        self.client = TestClient(load_app("trade-finance-service"))
        import trade_finance_service.database as db_mod

        fake_path = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
            "trade-finance-service",
            "fake_neo4j.py",
        )
        spec = importlib.util.spec_from_file_location("upg_tf_fake", fake_path)
        fake = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(fake)
        session = fake.FakeSession()
        db_mod.Neo4jConnector.get_driver = classmethod(lambda cls: fake.FakeDriver(session))
        yield
        session.nodes.clear()
        session.edges.clear()

    def test_lc_lifecycle(self):
        inst = self.client.post(
            "/instruments",
            json={
                "company_id": "comp-1",
                "instrument_type": "letter_of_credit",
                "counterparty": "Overseas Supplier",
                "amount": 200000,
                "currency": "USD",
                "issuing_bank": "Stanbic",
            },
            headers=self.H,
        )
        data = inst.json()
        assert inst.status_code == 200
        assert data["fee_estimate"] == 400  # 200000 * 0.002
        assert data["risk_assessment"] == "medium"
        inst_id = data["id"]

        presented = self.client.post(f"/instruments/{inst_id}/present", params={"company_id": "comp-1"}, headers=self.H)
        assert presented.json()["status"] == "presented"

        settled = self.client.post(f"/instruments/{inst_id}/settle", params={"company_id": "comp-1"}, headers=self.H)
        assert settled.json()["status"] == "paid"


class TestWorkingCapitalService:
    def setup_method(self):
        self.client = TestClient(load_app("working-capital-finance-service"))

    def test_analysis(self):
        resp = self.client.post(
            "/analyze",
            json={
                "company_id": "comp-1",
                "period": "2026-Q1",
                "current_assets": 500000,
                "current_liabilities": 300000,
                "inventory": 150000,
                "accounts_receivable": 200000,
                "accounts_payable": 100000,
                "annual_revenue": 2000000,
                "cogs": 1200000,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["working_capital"] == 200000
        assert data["current_ratio"] == 1.67
        assert data["dso"] > 0
        assert data["cash_conversion_cycle"] > 0
        assert len(data["recommendation"]) > 0

    def test_factoring(self):
        resp = self.client.post(
            "/factoring",
            json={
                "company_id": "comp-1",
                "invoice_amount": 100000,
                "advance_rate": 0.85,
                "factor_fee_rate": 0.03,
                "discount_rate": 0.02,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["advance_amount"] == 85000
        assert data["fee"] == 3000
        assert data["discount"] == 2000


class TestTargetCostingService:
    def setup_method(self):
        self.client = TestClient(load_app("target-costing-service"))

    def test_target_cost(self):
        resp = self.client.post(
            "/calculate",
            json={
                "company_id": "comp-1",
                "product_name": "WidgetX",
                "target_selling_price": 100,
                "desired_profit_margin_pct": 20,
                "current_cost": 90,
                "component_costs": {"materials": 30, "labour": 25, "overhead": 20, "packaging": 15},
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["target_cost"] == 80  # 100 - 20
        assert data["cost_reduction_needed"] == 10
        assert data["cost_reduction_pct"] == 11.11
        assert data["feasible"] is True
        assert len(data["component_analysis"]) == 4


class TestThroughputAccountingService:
    def setup_method(self):
        self.client = TestClient(load_app("throughput-accounting-service"))

    def test_optimal_mix(self):
        resp = self.client.post(
            "/analyze",
            json={
                "company_id": "comp-1",
                "operating_expenses": 5000,
                "products": [
                    {
                        "name": "Product A",
                        "selling_price": 100,
                        "material_cost": 40,
                        "time_on_constraint": 2,
                        "demand": 100,
                    },
                    {
                        "name": "Product B",
                        "selling_price": 80,
                        "material_cost": 20,
                        "time_on_constraint": 4,
                        "demand": 50,
                    },
                ],
                "available_constraint_minutes": 240,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_throughput"] > 0
        # Product A has higher throughput per minute (30 vs 15)
        assert data["product_ranking"][0]["name"] == "Product A"
        assert data["constraint_utilization"] > 0


class TestEquivalentUnitsService:
    def setup_method(self):
        self.client = TestClient(load_app("equivalent-units-service"))

    def test_weighted_average(self):
        resp = self.client.post(
            "/calculate",
            json={
                "company_id": "comp-1",
                "department": "Assembly",
                "period": "2026-01",
                "method": "weighted_average",
                "units_started": 10000,
                "units_completed": 9000,
                "ending_wip_units": 1000,
                "ending_wip_completion_materials": 1.0,
                "ending_wip_completion_conversion": 0.5,
                "materials_cost_beginning": 5000,
                "conversion_cost_beginning": 3000,
                "materials_cost_added": 95000,
                "conversion_cost_added": 47000,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["equivalent_units_materials"] == 10000  # 9000 + 1000*1.0
        assert data["equivalent_units_conversion"] == 9500  # 9000 + 1000*0.5
        assert data["cost_per_unit_materials"] > 0
        assert data["cost_of_completed"] > 0
        assert data["cost_of_ending_wip"] > 0


class TestSegmentReportingService:
    def setup_method(self):
        self.client = TestClient(load_app("segment-reporting-service"))

    def test_report(self):
        resp = self.client.post(
            "/report",
            json={
                "company_id": "comp-1",
                "fiscal_year": 2026,
                "segments": [
                    {
                        "name": "Retail",
                        "segment_type": "business",
                        "revenue": 600000,
                        "expenses": 400000,
                        "profit_or_loss": 200000,
                        "assets": 500000,
                    },
                    {
                        "name": "Wholesale",
                        "segment_type": "business",
                        "revenue": 300000,
                        "expenses": 250000,
                        "profit_or_loss": 50000,
                        "assets": 200000,
                    },
                    {
                        "name": "Other",
                        "segment_type": "business",
                        "revenue": 50000,
                        "expenses": 45000,
                        "profit_or_loss": 5000,
                        "assets": 30000,
                    },
                ],
                "reconciliation_revenue": 20000,
                "reconciliation_expenses": 15000,
            },
        )
        data = resp.json()
        assert resp.status_code == 200
        assert data["total_revenue"] == 950000
        assert data["reportable_segments"] >= 2  # Retail and Wholesale should be reportable
        assert len(data["disclosure_notes"]) > 0
