"""Parity between the assurance seam and the services that cross it.

The R&D credit and the balanced scorecard now run either engine. These tests hold
both to the same figures, because a seam only earns its place if it reproduces what
the service already reported.
"""

import importlib.util
import os
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
    )
):
    pytest.skip("the installed extension predates these functions", allow_module_level=True)


def _client(service_dir, name):
    """Load a service by path with its own directory importable.

    Each service has its own calc.py, so the directory goes on the path and the
    cached `calc` module is dropped first; otherwise the second service would
    silently import the first one's calculator.
    """
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
