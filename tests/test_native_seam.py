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


# --- fixed assets -----------------------------------------------------------
# The register report is the last Python-only calculator in the assets family:
# a book summary, a ten-asset schedule, category totals and a capex split.

FIXED_ASSETS_REGISTER = {
    "company_id": "c1",
    "capital_budget": 200000.0,
    "target_capitalization_rate": 0.4,
    "assets": [
        {
            "asset_id": "a1",
            "asset_name": "Truck",
            "category": "vehicle",
            "purchase_date": "2022-01-01",
            "purchase_cost": 60000.0,
            "salvage_value": 6000.0,
            "useful_life_years": 5,
            "depreciation_method": "straight_line",
            "accumulated_depreciation": 36000.0,
        },
        {
            "asset_id": "a2",
            "asset_name": "Rig",
            "category": "equipment",
            "purchase_date": "2024-01-01",
            "purchase_cost": 30000.0,
            "salvage_value": 3000.0,
            "useful_life_years": 10,
            "depreciation_method": "straight_line",
            "accumulated_depreciation": 5400.0,
        },
        {
            "asset_id": "a3",
            "asset_name": "Old rig",
            "category": "equipment",
            "purchase_date": "2018-01-01",
            "purchase_cost": 10000.0,
            "salvage_value": 1000.0,
            "useful_life_years": 5,
            "depreciation_method": "straight_line",
            "accumulated_depreciation": 9000.0,
        },
    ],
}


def _fixed_assets_pair(request):
    import json

    client = TestClient(load_service("fixed-assets-service").main.app)
    served = client.post("/analyze", json=request)
    assert served.status_code == 200, served.text
    return served.json(), json.loads(native.fixed_assets_json(json.dumps(request)))


def test_fixed_assets_matches_the_service():
    service, rust = _fixed_assets_pair(FIXED_ASSETS_REGISTER)

    assert service["asset_summary"]["total_assets"] == rust["asset_summary"]["total_assets"]
    for key in ("total_cost", "net_book_value"):
        assert Decimal(str(service["asset_summary"][key])) == Decimal(str(rust["asset_summary"][key])), key

    assert len(service["depreciation_schedule"]) == len(rust["depreciation_schedule"])
    for served_row, rust_row in zip(service["depreciation_schedule"], rust["depreciation_schedule"]):
        assert served_row["asset_id"] == rust_row["asset_id"]
        for key in ("net_book_value", "remaining_life_years", "annual_depreciation"):
            assert Decimal(str(served_row[key])) == Decimal(str(rust_row[key])), key

    for served_row, rust_row in zip(service["asset_category_analysis"], rust["asset_category_analysis"]):
        assert served_row["category"] == rust_row["category"]
        for key in ("total_cost", "accumulated_dep", "net_value"):
            assert Decimal(str(served_row[key])) == Decimal(str(rust_row[key])), key

    for key in ("capital_budget", "replacement_needs", "expansion_allocations"):
        assert Decimal(str(service["capital_expenditure_plan"][key])) == Decimal(
            str(rust["capital_expenditure_plan"][key])
        ), key

    assert service["recommendations"] == rust["recommendations"]


def test_fixed_assets_flags_an_aged_register_on_both_sides():
    import copy

    request = copy.deepcopy(FIXED_ASSETS_REGISTER)
    request["assets"][0]["accumulated_depreciation"] = 60000.0
    service, rust = _fixed_assets_pair(request)

    assert service["recommendations"] == rust["recommendations"]
    assert "High accumulated depreciation - plan for asset renewal" in rust["recommendations"]


# --- depreciation and amortisation -----------------------------------------
# Both families were already in the assets crate, but the services still owned
# the public endpoints, so they only retire once the seam reproduces them.

DEPRECIATION_ASSETS = [
    {"asset_id": "a1", "cost": 60000.0, "salvage": 6000.0, "useful_life_years": 5},
    {"asset_id": "a2", "cost": 30000.0, "salvage": 0.0, "useful_life_years": 10},
]


def test_depreciation_batch_matches_the_service():
    import json

    client = TestClient(load_service("depreciation-service").main.app)
    request = {
        "company_id": "c1",
        "period_start": "2026-01-01",
        "period_end": "2026-12-31",
        "method": "declining_balance",
        "assets": DEPRECIATION_ASSETS,
    }
    served = client.post("/calculate", json=request)
    assert served.status_code == 200, served.text
    service = served.json()

    rust = json.loads(native.depreciation_json(json.dumps(DEPRECIATION_ASSETS), "declining_balance"))
    assert Decimal(str(service["total_depreciation"])) == Decimal(str(rust["total_depreciation"]))
    for served_row, rust_row in zip(service["asset_depreciation"], rust["asset_depreciation"]):
        assert served_row["asset_id"] == rust_row["asset_id"]
        for key in ("annual_depreciation", "accumulated"):
            assert Decimal(str(served_row[key])) == Decimal(str(rust_row[key])), key

    # The five-year schedule repeats total * 0.9 in both implementations.
    assert [row["depreciation"] for row in service["schedule"]] == [row["depreciation"] for row in rust["schedule"]]


def test_depreciation_method_matches_the_service():
    import json

    client = TestClient(load_service("depreciation-service").main.app)
    request = {
        "method": "double_declining",
        "cost": 60000.0,
        "salvage_value": 6000.0,
        "useful_life": 5,
    }
    service = client.post("/method", json=request).json()
    rust = json.loads(native.depreciation_method_json(60000.0, 6000.0, 5, "double_declining"))

    for key in ("annual_depreciation", "monthly_depreciation"):
        assert Decimal(str(service[key])) == Decimal(str(rust[key])), key
    # The crate names the ladder `schedule`; the service calls it
    # `depreciation_schedule`.
    assert len(service["depreciation_schedule"]) == len(rust["schedule"])
    for served_row, rust_row in zip(service["depreciation_schedule"], rust["schedule"]):
        assert served_row["year"] == rust_row["year"]
        for key in ("depreciation", "accumulated", "book_value"):
            assert Decimal(str(served_row[key])) == Decimal(str(rust_row[key])), key


def test_amortization_matches_the_service():
    import json

    client = TestClient(load_service("amortization-service").main.app)
    request = {
        "company_id": "c1",
        "period_start": "2026-01-01",
        "period_end": "2026-12-31",
        "intangibles": [
            {
                "intangible_id": "i1",
                "type": "software",
                "cost": 120000.0,
                "residual_value": 0.0,
                "useful_life_years": 5,
                "accumulated_amortization": 40000.0,
                "method": "reducing_balance",
                "rate": 0.25,
            },
            {
                "intangible_id": "i2",
                "type": "patent",
                "cost": 50000.0,
                "residual_value": 5000.0,
                "useful_life_years": 9,
                "accumulated_amortization": 15000.0,
                "method": "straight_line",
                "rate": 0.0,
            },
        ],
    }
    service = client.post("/calculate", json=request).json()
    rust = json.loads(native.amortization_json(json.dumps(request["intangibles"])))

    for key in ("total_amortization", "total_net_book_value"):
        assert Decimal(str(service[key])) == Decimal(str(rust[key])), key

    assert len(service["schedules"]) == len(rust["schedules"])
    for served_row, rust_row in zip(service["schedules"], rust["schedules"]):
        assert served_row["intangible_id"] == rust_row["intangible_id"]
        for key in (
            "annual_amortization",
            "monthly_amortization",
            "accumulated_amortization",
            "net_book_value",
            "remaining_life_years",
        ):
            assert Decimal(str(served_row[key])) == Decimal(str(rust_row[key])), key


# --- investor return metrics ------------------------------------------------
# Five small calculators that decide a rating or a recommendation as well as a
# number, so the commentary is compared too, not just the ratios.

DUPONT_REQUEST = {
    "company_id": "c1",
    "period": "FY2026",
    "net_income": 180000.0,
    "revenue": 1200000.0,
    "total_assets": 900000.0,
    "total_equity": 400000.0,
    "ebt": 240000.0,
    "ebit": 300000.0,
    "total_debt": 300000.0,
}

TURNOVER_REQUEST = {
    "company_id": "c1",
    "period": "FY2026",
    "revenue": 1200000.0,
    "cost_of_goods_sold": 700000.0,
    "total_assets": 900000.0,
    "fixed_assets": 500000.0,
    "current_assets": 400000.0,
    "current_liabilities": 250000.0,
    "accounts_receivable": 180000.0,
    "inventory": 140000.0,
    "accounts_payable": 95000.0,
}

CROIC_REQUEST = {
    "company_id": "c1",
    "operating_cash_flow": 210000.0,
    "capital_expenditures": 90000.0,
    "total_assets": 900000.0,
    "current_liabilities": 250000.0,
    "tax_rate": 0.25,
    "periods": ["2022", "2023", "2024", "2025", "2026"],
}

EVA_REQUEST = {
    "company_id": "c1",
    "period": "FY2026",
    "ebit": 300000.0,
    "tax_rate": 0.25,
    "total_assets": 900000.0,
    "current_liabilities": 250000.0,
    "debt": 300000.0,
    "equity": 400000.0,
    "cost_of_debt": 0.08,
    "cost_of_equity": 0.14,
    "expected_growth": 0.05,
}

EPS_REQUEST = {
    "company_id": "c1",
    "period": "FY2026",
    "net_profit_attributable": 180000.0,
    "weighted_average_shares": 100000.0,
    "discontinued_operations_profit": 20000.0,
    "potential_dilutive_shares": [
        {
            "id": "opt1",
            "number_of_options": 12000.0,
            "exercise_price": 15.0,
            "average_market_price": 20.0,
        },
        {
            "id": "opt2",
            "number_of_options": 5000.0,
            "exercise_price": 25.0,
            "average_market_price": 20.0,
        },
    ],
    "anti_dilutive_items": [{"anti_dilutive_shares": 1500.0}],
}


def _whole_body_parity(service_name, path, request, seam_call):
    """Every key the service publishes must survive the round trip."""
    import json

    client = TestClient(load_service(service_name).main.app)
    served = client.post(path, json=request)
    assert served.status_code == 200, served.text

    rust = json.loads(seam_call(json.dumps(request)))
    return served.json(), rust


def test_du_pont_matches_the_service():
    service, rust = _whole_body_parity("du-pont-service", "/analyze", DUPONT_REQUEST, native.du_pont_json)
    assert service["du_pont_metrics"] == rust["du_pont_metrics"]
    assert service["three_factor_breakdown"] == rust["three_factor_breakdown"]
    assert service["five_factor_breakdown"] == rust["five_factor_breakdown"]
    assert service["peer_comparison"] == rust["peer_comparison"]
    assert service["conclusions"] == rust["conclusions"]


def test_asset_turnover_matches_the_service():
    service, rust = _whole_body_parity(
        "asset-turnover-service", "/analyze", TURNOVER_REQUEST, native.asset_turnover_json
    )
    assert service["turnover_metrics"] == rust["turnover_metrics"]
    assert service["industry_benchmark"] == rust["industry_benchmark"]
    assert service["efficiency_assessment"] == rust["efficiency_assessment"]
    assert service["improvement_areas"] == rust["improvement_areas"]
    assert service["recommendations"] == rust["recommendations"]


def test_croic_matches_the_service():
    service, rust = _whole_body_parity("croic-service", "/calculate", CROIC_REQUEST, native.croic_json)
    assert service["current_croic"] == rust["current_croic"]
    assert service["historical_croic"] == rust["historical_croic"]
    assert service["croic_vs_industry"] == rust["croic_vs_industry"]
    assert service["recommendations"] == rust["recommendations"]


def test_eva_matches_the_service():
    service, rust = _whole_body_parity("eva-service", "/calculate", EVA_REQUEST, native.eva_json)
    assert service["eva_metrics"] == rust["eva_metrics"]
    assert service["eva_by_segment"] == rust["eva_by_segment"]
    assert service["eva_momentum"] == rust["eva_momentum"]
    assert service["wealth_created"] == rust["wealth_created"]
    assert service["recommendations"] == rust["recommendations"]


def test_eps_matches_the_service():
    service, rust = _whole_body_parity("eps-service", "/calculate", EPS_REQUEST, native.eps_json)
    for key in (
        "basic_eps",
        "diluted_eps",
        "eps_from_continuing_operations",
        "weighted_average_shares",
        "diluted_shares",
        "dilutive_effect",
        "anti_dilutive_effect",
    ):
        assert Decimal(str(service[key])) == Decimal(str(rust[key])), key
    assert service["potential_dilutive_instruments"] == rust["potential_dilutive_instruments"]
