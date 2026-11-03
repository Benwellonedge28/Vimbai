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
    for name in ("tax_credit_json", "scorecard_json", "vat_return_json", "grant_json", "controls_test_json")
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
