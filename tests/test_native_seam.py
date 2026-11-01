"""Parity between the Rust calculation seam and the Python services.

The seam only earns trust if it reproduces what the services already report,
so these tests run the same request through both and compare the figures
instead of checking that a call succeeded.
"""

import os
import sys
from decimal import Decimal

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service
from tests.test_treasury_services import analytics_client  # noqa: F401 - fixture

native = pytest.importorskip("vimbai_native", reason="build library/native with maturin to test the seam")


def _amounts(response_payload, keys):
    return {key: Decimal(response_payload[key]) for key in keys}


RATIOS = ("cash_adequacy_days", "debt_service_ratio", "investment_yield", "fx_risk_score")


def _assert_parity(client, request):
    """The service and the Rust seam must agree on every figure they report."""
    served = client.post("/analyze", json=request)
    assert served.status_code == 200, served.text
    service = served.json()

    rust = native.analyze_treasury_json(__import__("json").dumps({k: str(v) for k, v in request.items()}))
    rust = __import__("json").loads(rust)

    assert _amounts(service, RATIOS) == _amounts(rust, RATIOS), request

    by_name = {kpi["name"]: kpi for kpi in rust["kpis"]}
    assert {kpi["name"] for kpi in service["kpis"]} == set(by_name)
    for kpi in service["kpis"]:
        twin = by_name[kpi["name"]]
        assert Decimal(kpi["value"]) == Decimal(twin["value"]), kpi["name"]
        assert kpi["status"] == twin["status"], kpi["name"]
        assert Decimal(str(kpi["benchmark"])) == Decimal(str(twin["benchmark"])), kpi["name"]


def test_healthy_book_matches_the_service(analytics_client):
    _assert_parity(
        analytics_client,
        {
            "company_id": "comp-1",
            "total_cash": 500000,
            "monthly_inflow": 200000,
            "monthly_outflow": 150000,
            "short_term_debt": 30000,
            "total_debt": 100000,
            "investments": 200000,
            "fx_exposure": 50000,
        },
    )


def test_no_burn_reports_the_runway_floor(analytics_client):
    _assert_parity(
        analytics_client,
        {
            "company_id": "comp-2",
            "total_cash": 250000,
            "monthly_inflow": 0,
            "monthly_outflow": 0,
            "short_term_debt": 0,
            "total_debt": 0,
            "investments": 0,
            "fx_exposure": 0,
        },
    )


def test_net_receivable_scores_negative_and_still_reads_good(analytics_client):
    """The source caps the FX score at 100 but never floors it."""
    _assert_parity(
        analytics_client,
        {
            "company_id": "comp-3",
            "total_cash": 100000,
            "monthly_inflow": 40000,
            "monthly_outflow": 90000,
            "short_term_debt": 80000,
            "total_debt": 300000,
            "investments": 10000,
            "fx_exposure": -50000,
        },
    )


def test_thin_book_is_reported_the_same_way(analytics_client):
    _assert_parity(
        analytics_client,
        {
            "company_id": "comp-4",
            "total_cash": 100,
            "monthly_inflow": 10,
            "monthly_outflow": 5000,
            "short_term_debt": 400,
            "total_debt": 900,
            "investments": 5000,
            "fx_exposure": 999,
        },
    )


def test_amounts_cross_the_boundary_unchanged():
    huge = "123456789012345678901234567890.55"
    assert Decimal(native.sum_amounts(["0.1", "0.2", huge])) == Decimal(huge) + Decimal("0.3")


def test_allocation_keeps_every_cent():
    parts = [Decimal(p) for p in native.allocate("100.00", ["1", "1", "1"])]
    assert sum(parts) == Decimal("100.00")
    assert parts == [Decimal("33.34"), Decimal("33.33"), Decimal("33.33")]


def test_the_seam_refuses_a_bad_amount():
    with pytest.raises(ValueError):
        native.sum_amounts(["not money"])


def test_shim_reports_the_extension_it_loaded():
    from shared import native_seam

    assert native_seam.available()
    assert native_seam.version() == native.__version__
    assert native_seam.sum_amounts(["0.1", "0.2"]) == Decimal("0.3")


BOOKS = [
    {
        "company_id": "comp-a",
        "total_cash": 500000,
        "monthly_inflow": 200000,
        "monthly_outflow": 150000,
        "short_term_debt": 30000,
        "total_debt": 100000,
        "investments": 200000,
        "fx_exposure": 50000,
    },
    {
        "company_id": "comp-b",
        "total_cash": 100,
        "monthly_inflow": 0,
        "monthly_outflow": 0,
        "short_term_debt": 0,
        "total_debt": 0,
        "investments": 0,
        "fx_exposure": 0,
    },
    {
        "company_id": "comp-c",
        "total_cash": 100000,
        "monthly_inflow": 40000,
        "monthly_outflow": 90000,
        "short_term_debt": 80000,
        "total_debt": 300000,
        "investments": 10000,
        "fx_exposure": -50000,
    },
]


@pytest.mark.parametrize("book", BOOKS, ids=lambda b: b["company_id"])
def test_python_engine_matches_rust_engine(analytics_client, monkeypatch, book):
    """The fallback and the calculator of record must not drift apart."""
    monkeypatch.setenv("VIMBAI_CALC_ENGINE", "rust")
    rust = analytics_client.post("/analyze", json=book).json()
    monkeypatch.setenv("VIMBAI_CALC_ENGINE", "python")
    python = analytics_client.post("/analyze", json=book).json()

    assert _amounts(rust, RATIOS) == _amounts(python, RATIOS)
    rust_kpis = {kpi["name"]: kpi for kpi in rust["kpis"]}
    for kpi in python["kpis"]:
        twin = rust_kpis[kpi["name"]]
        assert Decimal(kpi["value"]) == Decimal(twin["value"]), kpi["name"]
        assert kpi["status"] == twin["status"], kpi["name"]


def test_health_says_which_calculator_answers(analytics_client, monkeypatch):
    monkeypatch.setenv("VIMBAI_CALC_ENGINE", "auto")
    assert analytics_client.get("/health").json()["calc_engine"] == "rust"
    monkeypatch.setenv("VIMBAI_CALC_ENGINE", "python")
    assert analytics_client.get("/health").json()["calc_engine"] == "python"


# --- tax family -------------------------------------------------------------
# corporate-tax-service and tax-calculation-service are the next two services
# the seam is meant to replace, so the same rule applies: the figures match or
# the seam does not ship.

CORPORATE_TAX_REQUEST = {
    "company_id": "c1",
    "fiscal_year": 2026,
    "accounting_profit": 500000.0,
    "adjustments": [
        {"type": "addition", "amount": 20000.0, "description": "disallowed"},
        {"type": "deduction", "amount": 5000.0, "description": "exempt"},
    ],
    "statutory_rate": 0.25,
    "credits": [{"amount": 3000.0, "description": "credit"}],
    "estimated_payments": 40000.0,
}


def test_corporate_tax_matches_the_service():
    import json

    client = TestClient(load_service("corporate-tax-service").main.app)
    served = client.post("/calculate", json=CORPORATE_TAX_REQUEST)
    assert served.status_code == 200, served.text
    service = served.json()

    rust = json.loads(native.corporate_tax_json(json.dumps(CORPORATE_TAX_REQUEST)))
    for key in (
        "taxable_income",
        "tax_before_credits",
        "total_credits",
        "net_tax_liability",
        "effective_rate",
        "balance_due",
    ):
        assert Decimal(str(service[key])) == Decimal(str(rust[key])), key

    assert len(service["installment_schedule"]) == len(rust["installment_schedule"])
    for served_row, rust_row in zip(service["installment_schedule"], rust["installment_schedule"]):
        assert served_row["quarter"] == rust_row["quarter"]
        assert served_row["due_date"] == rust_row["due_date"]
        assert Decimal(str(served_row["amount"])) == Decimal(str(rust_row["amount"]))


def test_corporate_tax_floors_a_loss_at_zero_on_both_sides():
    import json

    request = dict(CORPORATE_TAX_REQUEST, accounting_profit=10000.0, credits=[], estimated_payments=0.0)
    request["adjustments"] = [{"type": "deduction", "amount": 25000.0, "description": "exempt"}]

    client = TestClient(load_service("corporate-tax-service").main.app)
    service = client.post("/calculate", json=request).json()
    rust = json.loads(native.corporate_tax_json(json.dumps(request)))

    assert Decimal(str(service["taxable_income"])) == Decimal("0")
    assert Decimal(str(rust["taxable_income"])) == Decimal(str(service["taxable_income"]))
    assert Decimal(str(service["balance_due"])) == Decimal(str(rust["balance_due"]))


def test_tax_calculation_matches_the_service():
    import json

    request = {
        "company_id": "c1",
        "tax_type": "income_tax",
        "country": "ZW",
        "taxable_amount": 120000.0,
        "deductions": 20000.0,
        "fiscal_year": 2026,
    }
    client = TestClient(load_service("tax-calculation-service").main.app)
    served = client.post("/calculate", json=request)
    assert served.status_code == 200, served.text
    service = served.json()

    rust = json.loads(native.tax_calculation_json(json.dumps(request)))
    for key in ("tax_owed", "effective_rate", "marginal_rate"):
        assert Decimal(str(service[key])) == Decimal(str(rust[key])), key
    assert set(service["breakdown"]) == set(rust["breakdown"])
    for bracket, amount in service["breakdown"].items():
        assert Decimal(str(amount)) == Decimal(str(rust["breakdown"][bracket])), bracket
