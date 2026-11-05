"""Parity between the assurance seam and the services that cross it.

The R&D credit and the balanced scorecard now run either engine. These tests hold
both to the same figures, because a seam only earns its place if it reproduces what
the service already reported.
"""

import importlib.util
import json
import os
import pathlib
import sys

import pytest
from fastapi.testclient import TestClient

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
native = pytest.importorskip("vimbai_native", reason="build library/native to test the seam")
if not all(
    hasattr(native, name)
    for name in (
        "tax_credit_json",
        "scorecard_json",
        "vat_return_json",
        "grant_json",
        "controls_test_json",
        "var_json",
        "regulatory_ratio_json",
        "distress_json",
        "tax_provision_json",
        "tax_risk_json",
        "group_tax_json",
        "partnership_allocation_json",
        "partner_buyout_json",
        "partnership_goodwill_json",
        "pension_json",
        "inventory_valuation_json",
        "nrv_json",
        "consolidation_json",
        "options_price_json",
        "fraud_risk_json",
        "du_pont_json",
    )
):
    pytest.skip("the installed extension predates these functions", allow_module_level=True)


def _client(service_dir, name):
    """Load a service by path with its own directory importable.

    Each service has its own calc.py, so the directory goes on the path and the
    cached `calc` module is dropped first; otherwise the second service would
    silently import the first one's calculator.
    """
    from tests.conftest import service_missing

    if service_missing(service_dir):
        pytest.skip(f"{service_dir} has been deleted; its maths lives in the Rust library")
    if REPO_ROOT not in sys.path:
        sys.path.insert(0, REPO_ROOT)
    sys.modules.pop("calc", None)
    service_path = os.path.join(REPO_ROOT, service_dir)
    sys.path.insert(0, service_path)
    try:
        spec = importlib.util.spec_from_file_location(name, os.path.join(service_path, "main.py"))
        module = importlib.util.module_from_spec(spec)
        sys.modules[name] = module
        spec.loader.exec_module(module)
    finally:
        sys.path.remove(service_path)
    return TestClient(module.app)


@pytest.fixture(autouse=True)
def _engine(monkeypatch):
    """Pin the Rust engine: these tests exist to prove it answers."""
    monkeypatch.setenv("VIMBAI_CALC_ENGINE", "rust")


def test_rd_credit_reports_the_rust_engine():
    client = _client("r-and-d-tax-service", "seam_rnd_main")
    health = client.get("/health").json()
    assert health["calculation_engine"] == "rust"


def test_rd_credit_matches_the_reference():
    payload = {
        "company_id": "comp-seam",
        "fiscal_year": 2026,
        "credit_rate": 0.12,
        "alternative_rate": 0.08,
        "expenditures": [
            {"category": "wages", "description": "engineers", "amount": 120000.0, "qualifies": True},
            {"category": "supplies", "description": "lab", "amount": 30000.5, "qualifies": True},
            {"category": "rent", "description": "office", "amount": 20000.0, "qualifies": False},
        ],
    }
    client = _client("r-and-d-tax-service", "seam_rnd_main")
    rust = client.post("/calculate", json=payload).json()

    os.environ["VIMBAI_CALC_ENGINE"] = "python"
    python = _client("r-and-d-tax-service", "seam_rnd_reference").post("/calculate", json=payload).json()
    os.environ["VIMBAI_CALC_ENGINE"] = "rust"

    for field in ("total_expenditure", "qualifying_expenditure", "non_qualifying", "expenditure_breakdown"):
        assert rust[field] == python[field], field
    # both engines compare the unrounded credits, so the recommendation agrees
    assert rust["recommended_method"] == python["recommended_method"]
    assert rust["credit_regular"] == pytest.approx(python["credit_regular"], abs=0.01)
    assert rust["credit_alternative"] == pytest.approx(python["credit_alternative"], abs=0.01)
    assert rust["credit_regular"] == 18000.06
    assert rust["recommended_method"] == "regular"


def test_scorecard_reports_the_rust_engine():
    client = _client("balanced-scorecard-service", "seam_bsc_main")
    assert client.get("/health").json()["calculation_engine"] == "rust"


def test_scorecard_matches_the_reference():
    payload = {
        "company_id": "comp-seam",
        "period": "2026-Q1",
        "kpis": [
            {
                "name": "Revenue growth",
                "perspective": "financial",
                "target": 1000000.0,
                "actual": 800000.0,
                "weight": 2.0,
            },
            {"name": "NPS", "perspective": "customer", "target": 50.0, "actual": 30.0, "weight": 1.0},
            {"name": "Cycle time", "perspective": "internal", "target": 10.0, "actual": 12.0, "weight": 1.0},
        ],
    }
    genesis = _client("balanced-scorecard-service", "seam_bsc_main")
    rust = genesis.post("/score", json=payload).json()

    os.environ["VIMBAI_CALC_ENGINE"] = "python"
    python = _client("balanced-scorecard-service", "seam_bsc_reference").post("/score", json=payload).json()
    os.environ["VIMBAI_CALC_ENGINE"] = "rust"

    for field in (
        "financial_score",
        "customer_score",
        "internal_score",
        "learning_score",
        "overall_score",
        "action_items",
    ):
        assert rust[field] == python[field], field
    # the scale quirk is preserved on both sides, not corrected by the port
    assert rust["financial_score"] == 8000.0
    # the internal KPI met its target, so it scores 10,000 and drags the mean to 6,000
    assert rust["internal_score"] == 10000.0
    assert rust["overall_score"] == 6000.0
    assert rust["action_items"][0] == "Excellent performance across all perspectives"


def test_a_missing_extension_refuses_to_start_when_rust_is_demanded():
    """VIMBAI_CALC_ENGINE=rust must fail loudly, not quietly fall back."""
    client = _client("r-and-d-tax-service", "seam_rnd_absent")
    calc_module = sys.modules["r_and_d_tax_calc"]
    original = calc_module._extension
    calc_module._extension = lambda: None
    try:
        with pytest.raises(calc_module.EngineUnavailable):
            client.get("/health")
    finally:
        calc_module._extension = original


def test_the_reference_engine_never_touches_the_extension(monkeypatch):
    """Pinning python must bypass Rust entirely, even when it is installed."""
    client = _client("balanced-scorecard-service", "seam_bsc_pinned")
    calc_module = sys.modules["balanced_scorecard_calc"]
    calls = []
    monkeypatch.setenv("VIMBAI_CALC_ENGINE", "python")
    monkeypatch.setattr(calc_module, "_extension", lambda: calls.append("touched"))
    payload = {
        "company_id": "comp-seam",
        "period": "2026-Q1",
        "kpis": [{"name": "R", "perspective": "financial", "target": 100.0, "actual": 80.0, "weight": 2.0}],
    }
    answer = client.post("/score", json=payload).json()
    assert answer["overall_score"] == 2000.0
    assert calls == []
    monkeypatch.setenv("VIMBAI_CALC_ENGINE", "rust")


VAT_PAYLOAD = {
    "company_id": "comp-seam",
    "tax_period": "2026-Q1",
    "output_transactions": [
        {"description": "sales", "amount": 100000.0, "vat_rate": 0.15, "transaction_type": "standard"},
        {"description": "exports", "amount": 40000.0, "vat_rate": 0.0, "transaction_type": "zero_rated"},
        {"description": "health", "amount": 10000.0, "vat_rate": 0.15, "transaction_type": "exempt"},
    ],
    "input_transactions": [
        {"description": "stock", "amount": 60000.0, "vat_rate": 0.15, "transaction_type": "standard"},
    ],
}

GRANT_PAYLOAD = {
    "company_id": "comp-seam",
    "grant_name": "Solar rooftop",
    "granting_authority": "Ministry of Finance",
    "grant_amount": 120000.0,
    "grant_type": "asset",
    "recognition_method": "deferred_income",
    "asset_useful_life": 4,
    "related_asset_cost": 500000.0,
    "conditions": ["retain jobs"],
    "compliance_status": "compliant",
}

CONTROLS_PAYLOAD = {
    "company_id": "comp-seam",
    "fiscal_year": 2026,
    "controls": [
        {
            "id": "c1",
            "name": "Bank reconciliation",
            "control_type": "preventive",
            "description": "d",
            "process": "cash",
            "test_sample_size": 40,
            "exceptions_found": 0,
        },
        {
            "id": "c2",
            "name": "Purchase approvals",
            "control_type": "detective",
            "description": "d",
            "process": "p2p",
            "test_sample_size": 40,
            "exceptions_found": 2,
        },
        {
            "id": "c3",
            "name": "Segregation of duties",
            "control_type": "preventive",
            "description": "d",
            "process": "p2p",
            "test_sample_size": 40,
            "exceptions_found": 4,
        },
        {
            "id": "c4",
            "name": "Month-end review",
            "control_type": "detective",
            "description": "d",
            "process": "gl",
            "test_sample_size": 40,
            "exceptions_found": 12,
        },
    ],
}


def _both_engines(service_dir, path, payload):
    """Post the same request through both engines and return the two answers."""
    genesis = _client(service_dir, f"{service_dir}_rust").post(path, json=payload).json()
    os.environ["VIMBAI_CALC_ENGINE"] = "python"
    reference = _client(service_dir, f"{service_dir}_reference").post(path, json=payload).json()
    os.environ["VIMBAI_CALC_ENGINE"] = "rust"
    return genesis, reference


def test_vat_return_matches_the_reference():
    rust, reference = _both_engines("vat-reporting-service", "/prepare-return", VAT_PAYLOAD)
    for field in (
        "total_output_vat",
        "total_input_vat",
        "net_vat_payable",
        "net_vat_refundable",
        "standard_rated_sales",
        "zero_rated_sales",
        "exempt_sales",
        "standard_rated_purchases",
        "vat_due_to_authority",
    ):
        assert rust[field] == reference[field], field
    assert rust["net_vat_payable"] == 7500.0
    assert rust["zero_rated_sales"] == 40000.0


def test_grant_recognition_matches_the_reference():
    rust, reference = _both_engines("government-grants-service", "/recognize", GRANT_PAYLOAD)
    for field in (
        "grant_amount",
        "annual_recognition",
        "annual_amortization",
        "deferred_income_balance",
        "journal_entries",
        "disclosure_notes",
        "compliance_status",
    ):
        assert rust[field] == reference[field], field
    assert rust["annual_recognition"] == 30000.0
    assert rust["deferred_income_balance"] == 90000.0


def test_control_testing_matches_the_reference():
    rust, reference = _both_engines("internal-controls-testing-service", "/test", CONTROLS_PAYLOAD)
    for field in (
        "total_controls",
        "controls_tested",
        "effective_controls",
        "deficient_controls",
        "material_weaknesses",
        "significant_deficiencies",
        "overall_assessment",
        "control_details",
        "remediation_plan",
    ):
        assert rust[field] == reference[field], field
    assert rust["overall_assessment"].startswith("Adverse opinion")
    assert [d["deficiency_level"] for d in rust["control_details"]] == [
        "none",
        "deficiency",
        "significant_deficiency",
        "material_weakness",
    ]


VAR_PAYLOAD = {
    "company_id": "comp-seam",
    "portfolio_name": "Treasury book",
    "positions": [
        {"instrument": "equities", "exposure": 500000.0, "volatility": 0.20},
        {"instrument": "fx", "exposure": 300000.0, "volatility": 0.10},
    ],
    "confidence_level": 0.95,
    "holding_period_days": 1,
}

RATIO_PAYLOAD = {
    "company_id": "comp-seam",
    "regulator_type": "bank",
    "tier1_capital": 900000.0,
    "tier2_capital": 100000.0,
    "risk_weighted_assets": 10000000.0,
    "total_capital": 1000000.0,
    "total_assets": 20000000.0,
    "liquid_assets": 600000.0,
    "net_loan": 450000.0,
    "regulatory_capital": {},
}

DISTRESS_COMPANY = {
    "company_id": "comp-seam",
    "company_name": "Retail Co",
    "industry": "retail",
    "working_capital": 400000.0,
    "total_assets": 2000000.0,
    "ebit": 300000.0,
    "total_equity": 800000.0,
    "total_liabilities": 1200000.0,
    "revenue": 5000000.0,
    "retained_earnings": 350000.0,
    "market_cap": 1800000.0,
    "current_assets": 700000.0,
    "current_liabilities": 300000.0,
}


def test_var_matches_the_reference():
    rust, reference = _both_engines("market-risk-service", "/var", VAR_PAYLOAD)
    for field in (
        "total_exposure",
        "portfolio_volatility",
        "var_95",
        "var_99",
        "expected_shortfall",
        "stress_loss_2sd",
        "stress_loss_3sd",
        "risk_level",
    ):
        assert rust[field] == reference[field], field
    # the flat 15% diversification haircut is preserved, not replaced by a matrix
    # 0.1625 weighted volatility, less the flat 15% diversification haircut
    assert rust["portfolio_volatility"] == 0.1381
    assert rust["risk_level"] == "high"


def test_regulatory_ratios_match_the_reference():
    rust, reference = _both_engines("regulatory-ratio-service", "/calculate", RATIO_PAYLOAD)
    for field in ("capital_ratios", "liquidity_ratios", "leverage_ratios", "overall_compliance", "recommendations"):
        assert rust[field] == reference[field], field
    assert rust["overall_compliance"] == "compliant"
    assert [r["ratio_name"] for r in rust["capital_ratios"]] == [
        "CET1 Ratio",
        "Tier 1 Ratio",
        "Total Capital Ratio",
    ]


def test_distress_prediction_matches_the_reference():
    rust, reference = _both_engines("financial-distress-prediction-service", "/predict", {"company": DISTRESS_COMPANY})
    for field in ("z_score", "zone", "probability_distress", "years_to_distress", "recommendation"):
        assert rust[field] == reference[field], field
    assert rust["z_score"] == 3.88
    assert rust["zone"] == "SAFE"

    # manufacturing reads the same score against a tighter safe threshold
    manufacturing = dict(DISTRESS_COMPANY, industry="manufacturing")
    rust, reference = _both_engines("financial-distress-prediction-service", "/predict", {"company": manufacturing})
    assert rust["probability_distress"] == reference["probability_distress"] == 0.03


PROVISION_PAYLOAD = {
    "company_id": "comp-seam",
    "fiscal_year": 2026,
    "pre_tax_income": 1000000.0,
    "statutory_rate": 0.25,
    "permanent_differences": 50000.0,
    "temp_differences": [
        {"description": "accelerated depreciation", "book_amount": 200000.0, "tax_amount": 150000.0, "type": "taxable"},
        {"description": "provision", "book_amount": 80000.0, "tax_amount": 0.0, "type": "deductible"},
    ],
    "loss_carryforward": 100000.0,
    "valuation_allowance": 20000.0,
}

TAX_RISK_PAYLOAD = {
    "company_id": "comp-seam",
    "risks": [
        {
            "risk_id": "r1",
            "description": "Transfer pricing exposure",
            "risk_type": "transfer_pricing",
            "potential_exposure": 400000.0,
            "probability": 0.6,
        },
        {
            "risk_id": "r2",
            "description": "VAT gap",
            "risk_type": "vat_gap",
            "potential_exposure": 120000.0,
            "probability": 0.3,
            "jurisdiction": "ZA",
        },
    ],
}

GROUP_TAX_PAYLOAD = {
    "group_id": "group-seam",
    "fiscal_year": 2026,
    "subsidiaries": [
        {
            "entity_id": "e1",
            "entity_name": "Profit Co",
            "jurisdiction": "ZW",
            "pre_tax_income": 500000.0,
            "tax_paid": 100000.0,
            "tax_rate": 0.25,
        },
        {
            "entity_id": "e2",
            "entity_name": "Loss Co",
            "jurisdiction": "ZW",
            "pre_tax_income": -200000.0,
            "tax_paid": 0.0,
            "tax_rate": 0.25,
        },
    ],
    "group_tax_rate": 0.25,
    "intercompany_eliminations": 0.0,
}


def test_tax_provision_matches_the_reference():
    rust, reference = _both_engines("tax-provision-service", "/calculate", PROVISION_PAYLOAD)
    for field in (
        "current_tax_expense",
        "deferred_tax_expense",
        "total_tax_expense",
        "effective_rate",
        "deferred_tax_assets",
        "deferred_tax_liabilities",
        "net_deferred_tax",
        "loss_carryforward_utilized",
    ):
        assert rust[field] == reference[field], field
    assert rust["current_tax_expense"] == 230000.0
    assert rust["net_deferred_tax"] == -12500.0


def test_tax_risk_assessment_matches_the_reference():
    rust, reference = _both_engines("tax-risk-service", "/assess", TAX_RISK_PAYLOAD)
    for field in ("overall_risk_level", "total_exposure", "weighted_exposure", "risk_count"):
        assert rust[field] == reference[field], field
    assert [r["level"] for r in rust["risks"]] == [r["level"] for r in reference["risks"]]
    # a risk's own jurisdiction and mitigation list survive the round trip
    assert rust["risks"][1]["jurisdiction"] == "ZA"
    assert rust["risks"][0]["mitigation_actions"] == []
    assert rust["overall_risk_level"] == "high"


def test_group_tax_consolidation_matches_the_reference():
    rust, reference = _both_engines("group-tax-service", "/consolidate", GROUP_TAX_PAYLOAD)
    for field in (
        "consolidated_income",
        "consolidated_tax",
        "total_tax_already_paid",
        "net_group_tax",
        "tax_savings_from_consolidation",
        "subsidiary_summary",
    ):
        assert rust[field] == reference[field], field
    # the loss-making entity shelters 200,000 of profit before the group rate applies
    assert rust["consolidated_income"] == 300000.0
    assert rust["tax_savings_from_consolidation"] == 25000.0


PARTNERS = [
    {
        "partner_id": "p1",
        "name": "A",
        "capital_contribution": 300000.0,
        "profit_share_pct": 60.0,
        "salary": 60000.0,
        "drawings": 20000.0,
    },
    {
        "partner_id": "p2",
        "name": "B",
        "capital_contribution": 200000.0,
        "profit_share_pct": 40.0,
        "salary": 40000.0,
        "drawings": 10000.0,
    },
]

BUYOUT_PAYLOAD = {
    "company_id": "comp-seam",
    "outgoing_partner_id": "p1",
    "outgoing_partner_name": "A",
    "capital_balance": 300000.0,
    "share_of_goodwill": 80000.0,
    "share_of_reserves": 20000.0,
    "loan_to_partner": 10000.0,
    "agreed_payment": 450000.0,
    "remaining_partners": [
        {"partner_id": "p2", "name": "B", "share_pct": 70},
        {"partner_id": "p3", "name": "C", "share_pct": 30},
    ],
}

PENSION_PAYLOAD = {
    "company_id": "comp-seam",
    "fiscal_year": 2026,
    "plan_name": "Main plan",
    "obligation_beginning": 2000000.0,
    "service_cost": 120000.0,
    "interest_cost": 100000.0,
    "benefits_paid": 80000.0,
    "actuarial_gain_loss": 30000.0,
    "past_service_cost": 0.0,
    "plan_assets_beginning": 1800000.0,
    "contributions": 150000.0,
    "expected_return": 90000.0,
    "actual_return": 95000.0,
    "benefits_paid_from_assets": 80000.0,
}


def test_partnership_allocation_matches_the_reference():
    payload = {
        "company_id": "comp-seam",
        "period": "2026",
        "partners": PARTNERS,
        "net_profit": 400000.0,
        "interest_rate_on_capital": 0.05,
    }
    rust, reference = _both_engines("partnership-accounting-service", "/allocate", payload)
    for field in ("total_salary", "total_interest", "total_profit_share", "residual_profit", "partner_accounts"):
        assert rust[field] == reference[field], field
    assert rust["residual_profit"] == 275000.0
    assert [a["closing_capital"] for a in rust["partner_accounts"]] == [
        300000.0 + 60000.0 + 15000.0 + 165000.0 - 20000.0,
        200000.0 + 40000.0 + 10000.0 + 110000.0 - 10000.0,
    ]


def test_partnership_allocation_falls_back_to_equal_shares():
    # every partner at 0% still gets a share, split equally. Salaries and capital
    # interest together absorb 125,000 of the 200,000, leaving 75,000 to divide.
    payload = {
        "company_id": "comp-seam",
        "period": "2026",
        "net_profit": 200000.0,
        "interest_rate_on_capital": 0.05,
        "partners": [{**p, "profit_share_pct": 0.0} for p in PARTNERS],
    }
    rust, reference = _both_engines("partnership-accounting-service", "/allocate", payload)
    assert rust["partner_accounts"] == reference["partner_accounts"]
    assert [a["profit_share"] for a in rust["partner_accounts"]] == [37500.0, 37500.0]


def test_partner_retirement_matches_the_reference():
    rust, reference = _both_engines("partnership-sale-service", "/retirement", BUYOUT_PAYLOAD)
    for field in (
        "capital_due",
        "goodwill_due",
        "reserves_due",
        "loan_due",
        "total_payable",
        "difference",
        "gain_or_loss_on_retirement",
        "remaining_partner_adjustments",
    ):
        assert rust[field] == reference[field], field
    # paid 40,000 more than the amount due, so the partners absorb a loss
    assert rust["difference"] == -40000.0
    assert [a["capital_adjustment"] for a in rust["remaining_partner_adjustments"]] == [
        -28000.0,
        -12000.0,
    ]


def test_partnership_goodwill_matches_the_reference():
    payload = {
        "company_id": "comp-seam",
        "avg_profit": 180000.0,
        "years": 3,
        "rate": 0.15,
        "partners": [
            {"partner_id": "p1", "capital": 300000.0, "share_pct": 60},
            {"partner_id": "p2", "capital": 200000.0, "share_pct": 40},
        ],
    }
    rust, reference = _both_engines("partnership-sale-service", "/goodwill", payload)
    for field in ("total_goodwill", "super_profit", "normal_return_rate", "partner_goodwill_shares"):
        assert rust[field] == reference[field], field
    assert rust["total_goodwill"] == 315000.0
    assert rust["partner_goodwill_shares"] == {"p1": 189000.0, "p2": 126000.0}


def test_pension_accounting_matches_the_reference():
    rust, reference = _both_engines("pension-accounting-service", "/calculate", PENSION_PAYLOAD)
    for field in (
        "dbo_end",
        "plan_assets_end",
        "funded_position",
        "net_defined_benefit_liability",
        "net_interest",
        "remeasurements",
        "total_pension_expense",
        "balance_sheet_entry",
    ):
        assert rust[field] == reference[field], field
    assert rust["funded_position"] == -205000.0
    assert rust["remeasurements"] == 35000.0


INVENTORY_PAYLOAD = {
    "company_id": "comp-seam",
    "purchases": [
        {"date": "2026-01-05", "quantity": 100, "unit_cost": 12.0},
        {"date": "2026-02-10", "quantity": 150, "unit_cost": 14.0},
    ],
    "sales": [{"date": "2026-03-01", "quantity": 200, "unit_price": 25.0}],
    "opening_inventory": 1000.0,
    "opening_qty": 80,
}


def test_inventory_valuation_matches_the_reference_in_every_method():
    expected = {"fifo": (2480.0, 1820.0), "lifo": (2700.0, 1600.0), "weighted_average": (2606.06, 1693.94)}
    for method, (cogs, ending) in expected.items():
        rust, reference = _both_engines(
            "inventory-valuation-service", "/calculate", {**INVENTORY_PAYLOAD, "method": method}
        )
        for field in (
            "total_goods_available",
            "total_units_available",
            "total_units_sold",
            "cost_of_goods_sold",
            "ending_inventory",
            "ending_qty",
            "total_sales_revenue",
            "gross_profit",
            "gross_margin",
        ):
            assert rust[field] == reference[field], (method, field)
        assert rust["cost_of_goods_sold"] == cogs, method
        assert rust["ending_inventory"] == ending, method


def test_net_realizable_value_matches_the_reference():
    rust, reference = _both_engines(
        "net-realizable-value-service",
        "/calculate-nrv?selling_Price=500&Completion_Cost=120&selling_cost=30",
        None,
    )
    assert rust == reference
    assert rust["net_realizable_value"] == 350.0
    assert rust["formula"] == "500.0 - 120.0 - 30.0 = 350.0"


CONSOLIDATION_PAYLOAD = {
    "parent_company_id": "parent-seam",
    "period": "2026",
    "subsidiaries": [
        {
            "subsidiary_id": "s1",
            "subsidiary_name": "Trading sub",
            "ownership_percentage": 80,
            "non_controlling_interests_percentage": 20,
            "financial_data": {
                "revenue": 2000000,
                "profit": 300000,
                "assets": 5000000,
                "liabilities": 2500000,
            },
        },
        {
            "subsidiary_id": "s2",
            "subsidiary_name": "Loss-making sub",
            "ownership_percentage": 100,
            "non_controlling_interests_percentage": 0,
            "financial_data": {
                "revenue": 1200000,
                "profit": -150000,
                "assets": 3000000,
                "liabilities": 1000000,
            },
        },
    ],
    "intercompany_transactions": [
        {"transaction_id": "t1", "transaction_type": "sale", "amount": 200000},
        {"transaction_id": "t2", "transaction_type": "dividend", "amount": 50000},
    ],
    "minority_interest": True,
}


def test_consolidation_matches_the_reference():
    rust, reference = _both_engines("consolidation-service", "/consolidate", CONSOLIDATION_PAYLOAD)
    assert rust == reference
    # a loss-making subsidiary drags the group profit down through its whole share
    assert rust["consolidated_profit"]["subsidiary_profits"] == {"s1": 240000.0, "s2": -150000.0}
    assert rust["consolidated_profit"]["nci_share"] == 9000.0
    assert rust["consolidated_profit"]["attributable_to_parent"] == 1581000.0
    # only sales are eliminated from revenue; the dividend is not
    assert rust["consolidated_revenue"]["intercompany_sales_elimination"] == 200000.0
    # 10,000,000 parent + 1,600,000 + 1,200,000 of subsidiary revenue at ownership share,
    # less 200,000 of intercompany sales
    assert rust["consolidated_revenue"]["consolidated_revenue"] == 12600000.0
    assert rust["elimination_count"] == 2


OPTIONS_PAYLOAD = {
    "company_id": "options-seam",
    "current_spot": 100.0,
    "options": [
        {
            "option_type": "call",
            "strike_price": 100.0,
            "spot_price": 100.0,
            "time_to_expiry": 1.0,
            "volatility": 0.2,
            "risk_free_rate": 0.05,
            "notional": 10000,
        },
        {
            "option_type": "put",
            "strike_price": 110.0,
            "spot_price": 100.0,
            "time_to_expiry": 0.5,
            "volatility": 0.25,
            "risk_free_rate": 0.03,
            "notional": 5000,
        },
        {
            "option_type": "call",
            "strike_price": 130.0,
            "spot_price": 100.0,
            "time_to_expiry": 0.25,
            "volatility": 0.35,
            "risk_free_rate": 0.04,
            "notional": 20000,
        },
    ],
}


def test_options_pricing_matches_the_reference():
    rust, reference = _both_engines("options-pricing-service", "/price", OPTIONS_PAYLOAD)
    assert rust == reference
    at_the_money, in_the_money, out_of_the_money = rust["option_valuations"]
    assert at_the_money["premium"] == 10.4506
    assert [v["moneyness"] for v in rust["option_valuations"]] == ["ATM", "ITM", "OTM"]
    # a long call and a long put carry opposite delta, so the book nets them off
    assert in_the_money["delta"] < 0 < at_the_money["delta"]
    assert rust["portfolio_summary"]["total_premium"] == 179036.03
    assert rust["greeks_summary"]["gamma"] > 0.1
    assert "Gamma risk elevated - monitor position closely" in rust["recommendations"]


FRAUD_PAYLOAD = {
    "audit_id": "fraud-seam",
    "company_id": "company-seam",
    "industry": "retail",
    "company_size": "small",
    "financial_indicators": {"revenue_growth": -0.2},
    "management_characteristics": ["high_turnover", "aggressive_targets"],
    "internal_control_gaps": ["segregation_of_duties", "no_review", "weak_it"],
    "prior_fraud_history": True,
}


def test_fraud_risk_matches_the_reference():
    rust, reference = _both_engines("fraud-risk-assessment-service", "/assess", FRAUD_PAYLOAD)
    assert rust == reference
    assert rust["fraud_risk_score"] == 46.0
    assert rust["risk_category"] == "medium"
    assert [i["indicator"] for i in rust["pressure_indicators"]] == [
        "Prior fraud incidents",
        "Limited resources",
        "Management turnover",
    ]
    assert rust["recommended_procedures"] == ["Standard substantive procedures"]
    assert rust["conclusion"] == "Fraud risk assessed as medium with score of 46.0"


def test_fraud_risk_reports_an_empty_side_of_the_triangle_as_a_place():
    quiet = dict(
        FRAUD_PAYLOAD,
        prior_fraud_history=False,
        company_size="large",
        management_characteristics=[],
        internal_control_gaps=[],
    )
    rust, reference = _both_engines("fraud-risk-assessment-service", "/assess", quiet)
    assert rust == reference
    assert rust["fraud_risk_score"] == 0.0
    assert rust["risk_category"] == "low"
    # each side still reports one zero-weight place, rather than an empty list
    assert rust["pressure_indicators"][0]["weight"] == 0.0
    assert rust["pressure_indicators"][0]["indicator"] == "No significant pressures identified"
    assert rust["high_risk_areas"] == ["No high risk areas identified"]


PERIOD = {"company_id": "ratio-seam", "period": "2026"}

RATIO_CASES = {
    "du-pont-service": (
        "/analyze",
        {
            **PERIOD,
            "revenue": 1e6,
            "net_income": 8e4,
            "ebt": 1e5,
            "ebit": 1.4e5,
            "total_assets": 2e6,
            "total_equity": 9e5,
            "total_debt": 6e5,
        },
        0.0889,
    ),
    "eva-service": (
        "/calculate",
        {
            **PERIOD,
            "total_assets": 2e6,
            "current_liabilities": 4e5,
            "equity": 9e5,
            "debt": 6e5,
            "cost_of_debt": 0.06,
            "cost_of_equity": 0.12,
            "ebit": 1.4e5,
            "tax_rate": 0.25,
            "expected_growth": 0.03,
        },
        None,
    ),
    "croic-service": (
        "/calculate",
        {
            **PERIOD,
            "periods": ["2026"],
            "operating_cash_flow": 3e5,
            "capital_expenditures": 1e5,
            "total_assets": 2e6,
            "current_liabilities": 4e5,
            "tax_rate": 0.25,
        },
        None,
    ),
    "asset-turnover-service": (
        "/analyze",
        {
            **PERIOD,
            "revenue": 1e6,
            "cost_of_goods_sold": 6e5,
            "total_assets": 2e6,
            "fixed_assets": 8e5,
            "current_assets": 1.2e6,
            "current_liabilities": 4e5,
            "accounts_receivable": 2e5,
            "inventory": 1.5e5,
            "accounts_payable": 1.2e5,
        },
        None,
    ),
}


@pytest.mark.parametrize("service", sorted(RATIO_CASES))
def test_ratio_services_match_the_reference(service):
    path, payload, expected_roe = RATIO_CASES[service]
    rust, reference = _both_engines(service, path, payload)
    assert rust == reference
    if expected_roe is not None:
        assert rust["du_pont_metrics"]["return_on_equity"] == expected_roe
    assert rust["company_id"] == "ratio-seam"


def test_du_pont_holds_the_three_factor_identity():
    """Margin times turnover times leverage is the ROE the endpoint reports."""
    _, payload, _ = RATIO_CASES["du-pont-service"]
    rust, _reference_answer = _both_engines("du-pont-service", "/analyze", payload)
    metrics = rust["du_pont_metrics"]
    product = metrics["net_profit_margin"] * metrics["asset_turnover"] * metrics["financial_leverage"]
    assert round(product, 4) == metrics["return_on_equity"]


def test_eps_matches_the_reference():
    payload = {
        "company_id": "eps-seam",
        "period": "2026",
        "net_profit_attributable": 8e5,
        "weighted_average_shares": 500000,
        "potential_dilutive_shares": [{"shares": 20000, "conversion_ratio": 0.5}],
        "anti_dilutive_items": [],
        "discontinued_operations_profit": 5e4,
    }
    rust, reference = _both_engines("eps-service", "/calculate", payload)
    assert rust == reference
    assert rust["weighted_average_shares"] == 500000
    # 800,000 / 500,000 basic; dilution can only ever lower it
    assert rust["basic_eps"] == 1.6
    assert rust["diluted_eps"] <= rust["basic_eps"]


def test_eps_treasury_shares_are_exact_not_float_truncated():
    """A documented divergence, kept visible rather than tolerated.

    The service computed the treasury-stock shares in floats:
    ``int(10000 * (1 - 40 / 50))`` is 1999, because 40/50 lands just under 0.8 in
    binary. The exact answer is 2000 shares. The library takes the exact one, so
    diluted shares and diluted EPS differ from the old service by that single share.
    Both numbers are asserted here, so the change can never happen silently.
    """
    payload = {
        "company_id": "eps-treasury",
        "period": "2026",
        "net_profit_attributable": 500000,
        "weighted_average_shares": 200000,
        "discontinued_operations_profit": 0,
        "potential_dilutive_shares": [
            {"id": "o1", "number_of_options": 10000, "exercise_price": 40, "average_market_price": 50}
        ],
        "anti_dilutive_items": [],
    }
    rust, reference = _both_engines("eps-service", "/calculate", payload)
    assert reference["dilutive_effect"] == 1999  # float truncation in the old service
    assert rust["dilutive_effect"] == 2000  # 20% of 10,000 options, exactly
    assert rust["diluted_shares"] == 202000
    assert reference["diluted_shares"] == 201999
    assert rust["basic_eps"] == reference["basic_eps"] == 2.5
    assert rust["period"] == reference["period"] == "2026"


def test_goodwill_impairment_matches_the_reference():
    payload = {
        "company_id": "goodwill-seam",
        "fiscal_year": 2026,
        "cgu": {
            "name": "North",
            "carrying_value": 1e6,
            "goodwill_allocated": 2e5,
            "fair_value": 9.5e5,
            "value_in_use": 9.8e5,
        },
        "test_method": "higher_of",
    }
    rust, reference = _both_engines("goodwill-service", "/test", payload)
    assert rust.pop("id") and reference.pop("id")
    assert rust == reference
    # value in use is the higher of the two, so the loss is the 20,000 gap
    assert rust["recoverable_amount"] == 9.8e5
    assert rust["impairment_loss"] == 2e4
    assert rust["goodwill_impaired"] == 2e4
    assert rust["is_impaired"] is True


TAX_PAYLOAD = {
    "company_id": "tax-seam",
    "fiscal_year": 2026,
    "accounting_profit": 1e6,
    "statutory_rate": 0.25,
    "adjustments": [{"description": "Disallowed fines", "amount": 20000, "type": "addition"}],
    "credits": [{"description": "R&D", "amount": 15000}],
    "estimated_payments": 1e5,
}


def test_corporate_tax_matches_the_reference():
    rust, reference = _both_engines("corporate-tax-service", "/calculate", TAX_PAYLOAD)
    rust.pop("id"), reference.pop("id")
    assert rust == reference
    # 1,000,000 + 20,000 at 25% = 255,000, less 15,000 credits
    assert rust["taxable_income"] == 1020000.0
    assert rust["tax_before_credits"] == 255000.0
    assert rust["net_tax_liability"] == 240000.0
    assert rust["balance_due"] == 140000.0
    assert [i["quarter"] for i in rust["installment_schedule"]] == [1, 2, 3, 4]


def test_corporate_tax_only_adds_on_the_word_addition():
    """A caller sending "add" gets a deduction. That is long-standing behaviour."""
    mislabelled = dict(
        TAX_PAYLOAD,
        adjustments=[
            {"description": "Disallowed fines", "amount": 20000, "type": "add"},
        ],
    )
    rust, reference = _both_engines("corporate-tax-service", "/calculate", mislabelled)
    rust.pop("id"), reference.pop("id")
    assert rust == reference
    assert rust["taxable_income"] == 980000.0


def test_corporate_tax_holds_a_zero_profit_year_together():
    flat = dict(TAX_PAYLOAD, accounting_profit=0, estimated_payments=0)
    rust, reference = _both_engines("corporate-tax-service", "/calculate", flat)
    rust.pop("id"), reference.pop("id")
    assert rust == reference
    assert rust["effective_rate"] == 0
    assert rust["balance_due"] == 0


def test_depreciation_ladder_matches_the_reference():
    payload = {"cost": 50000, "salvage_value": 5000, "useful_life": 5, "method": "straight_line"}
    rust, reference = _both_engines("depreciation-service", "/method", payload)
    assert rust == reference
    assert rust["annual_depreciation"] == 9000.0
    assert rust["monthly_depreciation"] == 750.0
    assert len(rust["depreciation_schedule"]) == 5
    assert rust["depreciation_schedule"][0]["book_value"] == 41000.0


def test_depreciation_double_declining_ignores_salvage_in_the_rate():
    """The rate is charged on full cost, not on cost less salvage - as it always was."""
    payload = {"cost": 50000, "salvage_value": 5000, "useful_life": 5, "method": "double_declining"}
    rust, reference = _both_engines("depreciation-service", "/method", payload)
    assert rust == reference
    assert rust["annual_depreciation"] == 20000.0


AMORTIZATION_ASSETS = [
    {
        "intangible_id": "i1",
        "type": "patent",
        "cost": 120000.0,
        "residual_value": 0.0,
        "useful_life_years": 10,
        "accumulated_amortization": 24000.0,
        "method": "straight_line",
        "rate": 0.25,
    },
    {
        "intangible_id": "i2",
        "type": "software",
        "cost": 60000.0,
        "residual_value": 5000.0,
        "useful_life_years": 5,
        "accumulated_amortization": 10000.0,
        "method": "reducing_balance",
        "rate": 0.3,
    },
]


def test_amortization_matches_the_reference():
    payload = {
        "company_id": "comp-seam",
        "intangibles": AMORTIZATION_ASSETS,
        "period_start": "2026-01-01",
        "period_end": "2026-12-31",
    }
    rust, reference = _both_engines("amortization-service", "/calculate", payload)
    assert rust["schedules"] == reference["schedules"]
    for field in ("total_amortization", "total_net_book_value", "period"):
        assert rust[field] == reference[field]

    # straight line divides the depreciable amount; reducing balance charges the
    # rate against cost less accumulated, so the two never coincide here
    patent, software = rust["schedules"]
    assert patent["annual_amortization"] == 12000.0
    assert software["annual_amortization"] == 15000.0
    assert rust["total_amortization"] == 27000.0


def test_amortization_holds_a_zero_life_intangible_at_zero():
    # a goodwill line with no useful life must not divide by zero or invent a charge
    payload = {
        "company_id": "comp-seam",
        "intangibles": [
            {
                "intangible_id": "i3",
                "type": "goodwill",
                "cost": 200000.0,
                "residual_value": 0.0,
                "useful_life_years": 0,
                "accumulated_amortization": 0.0,
                "method": "straight_line",
                "rate": 0.25,
            }
        ],
        "period_start": "2026-01-01",
        "period_end": "2026-06-30",
    }
    rust, reference = _both_engines("amortization-service", "/calculate", payload)
    # the response carries a fresh id per request; everything else must agree
    for answer in (rust, reference):
        answer.pop("id", None)
    assert rust == reference
    assert rust["schedules"][0]["annual_amortization"] == 0.0
    assert rust["schedules"][0]["remaining_life_years"] == 0.0


def _credit_payload(**overrides):
    borrower = {
        "borrower_id": "borrower-seam",
        "borrower_name": "Seam Ltd",
        "credit_facility_id": "facility-seam",
        "facility_type": "term_loan",
        "total_exposure": 1e6,
        "current_exposure": 8e5,
        "undrawn_committed": 2e5,
        "collateral_value": 4e5,
        "guarantee_value": 1e5,
        "repayment_terms": "monthly",
    }
    financials = {
        "borrower_id": "borrower-seam",
        "debt_to_equity": 1.2,
        "debt_to_ebitda": 2.5,
        "interest_coverage": 4.0,
        "current_ratio": 1.8,
        "quick_ratio": 1.1,
        "profit_margin": 0.12,
        "operating_cash_flow": 2e5,
        "free_cash_flow": 1.2e5,
    }
    qualitative = {
        "borrower_id": "borrower-seam",
        "industry_outlook": "stable",
        "management_quality": "good",
        "market_position": "strong",
        "regulatory_environment": "stable",
        "competitive_advantage": "brand",
    }
    borrower.update(overrides.get("borrower", {}))
    financials.update(overrides.get("financials", {}))
    qualitative.update(overrides.get("qualitative", {}))
    return {"borrower": borrower, "financials": financials, "qualitative": qualitative}


def test_credit_risk_matches_the_reference():
    rust, reference = _both_engines("credit-risk-analysis-service", "/analyze", _credit_payload())
    assert rust == reference
    assert rust["credit_rating"] == "A"
    assert rust["pd_calculations"]["one_year_pd"] == 0.001


def test_credit_risk_agrees_on_a_distressed_borrower():
    payload = _credit_payload(
        financials={"debt_to_equity": 3.5, "interest_coverage": 1.0, "current_ratio": 0.9, "free_cash_flow": -5e4},
        qualitative={"industry_outlook": "negative", "management_quality": "poor", "market_position": "weak"},
    )
    rust, reference = _both_engines("credit-risk-analysis-service", "/analyze", payload)
    assert rust == reference
    assert rust["credit_rating"] == "CCC"
    assert rust["recommendation"] == "DECLINE"


def test_credit_risk_endpoint_no_longer_fails_on_its_own_grade():
    # PDCalculation declared rating_grade a string and was handed an integer, so every
    # call failed validation. The field is the number it was always given.
    payload = _credit_payload()
    client = _client("credit-risk-analysis-service", "credit_risk_grade")
    response = client.post("/analyze", json=payload)
    assert response.status_code == 200
    body = response.json()
    assert isinstance(body["pd_calculations"]["rating_grade"], int)
    assert body["pd_calculations"]["rating_grade"] == body["rating_grade"]


CONTROL_PAYLOAD = {
    "audit_id": "a1",
    "company_id": "c1",
    "control_framework": "coso",
    "testing_approach": "substantive",
    "controls_to_test": [
        {"id": "C1", "description": "Segregation of duties", "type": "preventive", "deficiency": "none"},
        {"id": "C2", "description": "Month-end review", "type": "detective", "deficiency": "significant"},
        {"id": "C3", "description": "Access control", "deficiency": "material"},
    ],
}


def test_internal_controls_match_the_reference():
    rust, reference = _both_engines("internal-control-evaluation-service", "/evaluate", CONTROL_PAYLOAD)
    assert rust == reference
    assert rust["controls_tested"] == 3
    assert rust["effective_controls"] == 1
    assert rust["material_weaknesses"] == 1
    assert rust["significant_deficiencies"] == 1
    assert rust["overall_assessment"] == "ineffective_with_deficiencies"
    # a control with no type is preventive, as the service has always defaulted it
    assert rust["control_test_results"][2]["control_type"] == "preventive"
    assert len(rust["remediation_recommendations"]) == 3


def test_a_deficiency_the_service_does_not_recognise_still_fails_the_control():
    payload = dict(
        CONTROL_PAYLOAD,
        audit_id="a2",
        controls_to_test=[{"id": "C9", "description": "Odd one", "deficiency": "minor"}],
    )
    rust, reference = _both_engines("internal-control-evaluation-service", "/evaluate", payload)
    assert rust == reference
    # no severity is attached, so the control fails without dragging the assessment down
    assert rust["effective_controls"] == 0
    assert rust["overall_assessment"] == "effective"
    assert rust["remediation_recommendations"] == ["Continue monitoring control effectiveness"]


def test_an_engagement_with_no_controls_tests_nothing():
    payload = dict(CONTROL_PAYLOAD, audit_id="a3", controls_to_test=[])
    rust, reference = _both_engines("internal-control-evaluation-service", "/evaluate", payload)
    assert rust == reference
    assert rust["controls_tested"] == 0
    assert rust["deficiencies_found"] == 0
    assert rust["overall_assessment"] == "effective"


BUDGET_PAYLOAD = {
    "company_id": "c1",
    "planning_period": "2026",
    "cost_of_capital": 0.1,
    "capital_budget_limit": 1000000.0,
    "projects": [
        {
            "id": "P1",
            "name": "Line A",
            "initial_investment": 400000.0,
            "cash_flows": [120000.0, 150000.0, 180000.0, 200000.0],
        },
        {"id": "P2", "name": "Line B", "initial_investment": 900000.0, "cash_flows": [200000.0, 200000.0, 200000.0]},
    ],
}

LIQUIDITY_PAYLOAD = {
    "company_id": "c1",
    "starting_cash": 100000.0,
    "credit_lines_available": 200000.0,
    "minimum_cash_requirement": 120000.0,
    "cash_flows": [
        {"date": "2026-01-31", "inflow": 150000.0, "outflow": 120000.0, "net": 30000.0},
        {"date": "2026-02-28", "inflow": 110000.0, "outflow": 160000.0, "net": -50000.0},
    ],
}


def test_capital_budgeting_matches_on_both_engines():
    rust, reference = _both_engines("capital-budgeting-service", "/analyze", BUDGET_PAYLOAD)
    assert rust == reference
    assert len(rust["projects_analyzed"]) == 2


def test_capital_budgeting_irr_is_an_approximation_not_a_root():
    """The reported IRR is cost of capital plus a tenth of the NPV ratio.

    It is not a solved internal rate of return, and P2 - which never recovers its
    cost inside the horizon - is still given a payback of the full three years.
    """
    rust, _ = _both_engines("capital-budgeting-service", "/analyze", BUDGET_PAYLOAD)
    by_id = {p["project_id"]: p for p in rust["projects_analyzed"]}
    p1, p2 = by_id["P1"], by_id["P2"]
    assert p1["irr"] == round(0.1 + p1["npv"] / 400000.0 * 0.1, 4)
    assert p2["payback_period"] == 3
    assert p1["accept"] is True and p2["accept"] is False


def test_liquidity_forecast_matches_on_both_engines():
    rust, reference = _both_engines("liquidity-forecast-service", "/forecast", LIQUIDITY_PAYLOAD)
    assert rust == reference


def test_liquidity_shortfall_is_reported_as_required_credit():
    rust, _ = _both_engines("liquidity-forecast-service", "/forecast", LIQUIDITY_PAYLOAD)
    gaps = rust["liquidity_gaps"]
    assert len(gaps) == 1
    assert gaps[0]["date"] == "2026-02-28"
    assert gaps[0]["shortfall"] == gaps[0]["required_credit"]


def _module(service_dir, name):
    """Load a service module by path, the same way _client does, and return it."""
    from tests.conftest import service_missing

    if service_missing(service_dir):
        pytest.skip(f"{service_dir} has been deleted; its maths lives in the Rust library")
    if REPO_ROOT not in sys.path:
        sys.path.insert(0, REPO_ROOT)
    sys.modules.pop("calc", None)
    service_path = os.path.join(REPO_ROOT, service_dir)
    sys.path.insert(0, service_path)
    try:
        spec = importlib.util.spec_from_file_location(name, os.path.join(service_path, "main.py"))
        module = importlib.util.module_from_spec(spec)
        sys.modules[name] = module
        spec.loader.exec_module(module)
    finally:
        sys.path.remove(service_path)
    return module


def _payload_from(model):
    """A request body built from the model itself, so a schema change cannot hide."""
    import typing

    def value_for(annotation, depth):
        origin = getattr(annotation, "__origin__", None)
        if origin in (list, typing.List):
            return [value_for(annotation.__args__[0], depth + 1)] * 2
        if origin in (dict, typing.Dict):
            return {}
        if hasattr(annotation, "model_fields") and depth < 3:
            return from_model(annotation, depth + 1)
        if annotation is int:
            return 12
        if annotation is float:
            return 123456.78
        if annotation is bool:
            return True
        return "sample"

    def from_model(cls, depth=0):
        return {
            name: (
                value_for(field.annotation, depth)
                if field.is_required()
                else field.get_default(call_default_factory=True)
            )
            for name, field in cls.model_fields.items()
        }

    return from_model(model)


@pytest.mark.parametrize(
    ("service", "path", "request_model"),
    [
        ("annual-report-service", "/prepare", "AnnualReportRequest"),
        ("fixed-assets-service", "/analyze", "FixedAssetsRequest"),
        ("liquidity-risk-service", "/analyze", "LiquidityAnalysisRequest"),
    ],
)
def test_statement_services_match_the_reference(service, path, request_model):
    module = _module(service, f"schema_{service.replace('-', '_')}")
    payload = _payload_from(getattr(module, request_model))
    rust, reference = _both_engines(service, path, payload)
    assert rust == reference


TARGET_COST_PAYLOAD = {
    "company_id": "c1",
    "product_name": "Widget",
    "target_selling_price": 100.0,
    "desired_profit_margin_pct": 20.0,
    "current_cost": 120.0,
    "component_costs": {"parts": 60.0, "labour": 40.0},
}


def test_target_costing_matches_the_reference():
    rust, reference = _both_engines("target-costing-service", "/calculate", TARGET_COST_PAYLOAD)
    rust.pop("id"), reference.pop("id")
    assert rust == reference
    assert rust["target_profit"] == 20.0
    assert rust["target_cost"] == 80.0
    assert rust["cost_reduction_pct"] == 33.33
    assert rust["feasible"] is False
    # the component order is the caller's, not the alphabet's
    assert [c["component"] for c in rust["component_analysis"]] == ["parts", "labour"]


def test_target_costing_holds_amounts_a_float_cannot():
    """The library computes in decimals; only the legacy float edge rounds."""
    import vimbai_native

    huge = {
        "company_id": "c1",
        "product_name": "Widget",
        "target_selling_price": "123456789012345678901234.56",
        "desired_profit_margin_pct": "12.5",
        "current_cost": "100000000000000000000000.00",
        "component_costs": {"parts": "1", "labour": "1"},
    }
    exact = vimbai_native.target_cost_json(json.dumps(huge))
    body = json.loads(exact)
    # 12.5% of the price, to the cent, from a number an f64 would have rounded
    assert body["target_profit"] == 15432098626543209862654.32
    assert body["target_cost"] == 108024690385802469038580.24


PAYMENTS_PAYLOAD = {
    "company_id": "c1",
    "available_cash": 100000.0,
    "credit_available": 50000.0,
    "payments": [
        {
            "payment_id": "p1",
            "amount": 50000.0,
            "due_date": "2026-03-31",
            "discount_available": 10.0,
            "discount_days": 10,
        },
        {
            "payment_id": "p2",
            "amount": 20000.0,
            "due_date": "2026-04-15",
            "discount_available": 500.0,
            "discount_days": 15,
        },
    ],
}


def test_payment_optimization_matches_the_reference():
    rust, reference = _both_engines("payment-optimization-service", "/optimize", PAYMENTS_PAYLOAD)
    assert rust == reference
    # 10 days of a 0.02% discount annualises at 0.73%, well under the ceiling
    assert rust["payment_schedule"][0]["action"] == "PAY_EARLY"
    # 15 days of 2.5% annualises at 60.83%, so it is left alone despite the cash
    assert rust["payment_schedule"][1]["action"] == "PAY_NORMAL"
    assert rust["savings_analysis"]["discount_utilization"] == 1.96


SUBSTANTIVE_PAYLOAD = {
    "audit_id": "a1",
    "account_id": "acc1",
    "assertions": ["existence", "valuation"],
    "sample_size": 4,
    "misstatement_threshold": 50.0,
    "population": [
        {"item_id": "1", "amount": 1000.0, "misstatement": 100.0},
        {"item_id": "2", "amount": 1000.0, "misstatement": 0.0},
        {"item_id": "3", "amount": 1000.0, "misstatement": 0.0},
        {"item_id": "4", "amount": 1000.0, "misstatement": 0.0},
        {"item_id": "5", "amount": 1000.0, "misstatement": 0.0},
    ],
}


def test_substantive_testing_matches_the_reference():
    rust, reference = _both_engines("substantive-testing-service", "/test", SUBSTANTIVE_PAYLOAD)
    assert rust == reference
    assert rust["total_misstatement"] == 100.0
    # five items sampled at four projects 1.25x, which is the service's rule
    assert rust["projected_misstatement"] == 125.0
    assert rust["exceptions_found"] == 1
    assert rust["conclusion"] == "qualified"


DISPOSAL_PAYLOAD = {
    "company_id": "c1",
    "subsidiary_name": "Sub",
    "disposal_method": "sale",
    "disposal_proceeds": 1000000.0,
    "disposal_costs": 50000.0,
    "carrying_value": 800000.0,
    "associated_goodwill": 0.0,
    "retained_interest_pct": 0.0,
    "retained_interest_fair_value": 0.0,
}


def test_divestiture_matches_the_reference():
    rust, reference = _both_engines("divestiture-service", "/dispose", DISPOSAL_PAYLOAD)
    rust.pop("id"), reference.pop("id")
    assert rust == reference
    assert rust["net_proceeds"] == 950000.0
    assert rust["gain_or_loss"] == 150000.0
    # the entry text is part of the contract, down to Python's float rendering
    assert rust["journal_entries"][0] == "Dr Cash/Bank (950000.0)"
    assert rust["journal_entries"][-1] == "Cr Gain on Disposal (150000.0)"


def test_divestiture_with_a_retained_interest_keeps_the_service_arithmetic():
    body = dict(DISPOSAL_PAYLOAD, retained_interest_pct=20.0, retained_interest_fair_value=250000.0)
    rust, reference = _both_engines("divestiture-service", "/dispose", body)
    rust.pop("id"), reference.pop("id")
    assert rust == reference
    assert rust["gain_or_loss"] == 150000.0
    # the retained interest is added on top without reducing the carrying value
    assert rust["total_gain_recognized"] == 400000.0


CAPEX_PAYLOAD = {
    "company_id": "c1",
    "budget_year": "2026",
    "total_capex_budget": 100000.0,
    "projects": [
        {
            "project_id": "b",
            "project_name": "B",
            "priority": 2,
            "initial_investment": 60000.0,
            "annual_cash_flow": 20000.0,
            "project_life": 5.0,
        },
        {
            "project_id": "a",
            "project_name": "A",
            "priority": 1,
            "initial_investment": 50000.0,
            "annual_cash_flow": 15000.0,
            "project_life": 5.0,
        },
    ],
}


def test_capex_budget_matches_the_reference():
    rust, reference = _both_engines("capital-expenditure-budget-service", "/prepare", CAPEX_PAYLOAD)
    assert rust == reference
    # priority 1 is funded first, and the second project no longer fits
    assert [p["project_id"] for p in rust["funded_projects"]] == ["a"]
    assert [p["project_id"] for p in rust["deferred_projects"]] == ["b"]
    assert rust["funded_projects"][0]["roi"] == 50.0
    # the year is text and stays text through the decimal seam
    assert rust["budget_year"] == "2026"
