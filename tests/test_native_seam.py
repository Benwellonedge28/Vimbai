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


# --- capital-return calculators ---------------------------------------------
# Cost of capital, discount factors, discounted payback and ARR are query-param
# endpoints, so each call is made with params on the service side and as plain
# arguments through the seam.


def _params_parity(service_name, path, params, seam_call):
    import json

    client = TestClient(load_service(service_name).main.app)
    served = client.post(path, params=params)
    assert served.status_code == 200, served.text
    return served.json(), json.loads(seam_call(*params.values()))


def test_cost_of_capital_matches_the_service():
    params = {"interest_rate": 0.12, "risk_premium": 0.03}
    service, rust = _params_parity("cost-of-capital-service", "/calculate", params, native.coc_calculate_json)
    assert service == rust


def test_wacc_matches_the_service():
    params = {
        "equity_value": 400000.0,
        "cost_of_equity": 0.14,
        "debt_value": 300000.0,
        "cost_of_debt": 0.08,
        "tax_rate": 25.0,
    }
    service, rust = _params_parity("cost-of-capital-service", "/wacc", params, native.coc_wacc_json)
    for key in ("total_capital", "after_tax_cost_debt", "equity_weight", "debt_weight"):
        assert Decimal(str(service[key])) == Decimal(str(rust[key])), key
    assert service["wacc"] == rust["wacc"]
    assert service["wacc_percent"] == rust["wacc_percent"]


def test_cagr_matches_the_service():
    params = {"present_value": 100000.0, "future_value": 161051.0, "years": 5}
    service, rust = _params_parity("cost-of-capital-service", "/compound-rate", params, native.coc_compound_rate_json)
    assert service["compound_rate"] == rust["compound_rate"]
    assert service["rate_percent"] == rust["rate_percent"]


def test_discount_factor_matches_the_service():
    service, rust = _params_parity(
        "discount-factor-service",
        "/calculate",
        {"rate": 0.1, "years": 3},
        native.df_calculate_json,
    )
    assert Decimal(str(service["discount_factor"])) == Decimal(str(rust["discount_factor"]))


def test_annuity_factor_matches_the_service():
    service, rust = _params_parity(
        "discount-factor-service",
        "/annuity-factor",
        {"rate": 0.1, "years": 5},
        native.df_annuity_json,
    )
    assert Decimal(str(service["annuity_factor"])) == Decimal(str(rust["annuity_factor"]))
    assert service["formula"] == rust["formula"]


def test_discount_factor_range_matches_the_service():
    service, rust = _params_parity(
        "discount-factor-service",
        "/table-range",
        {"rate_start": 0.05, "rate_end": 0.08, "rate_step": 0.01, "years": 3},
        native.df_table_range_json,
    )
    assert len(service["factors"]) == len(rust["factors"])
    for served_row, rust_row in zip(service["factors"], rust["factors"]):
        assert served_row["rate"] == rust_row["rate"]
        assert Decimal(str(served_row["discount_factor"])) == Decimal(str(rust_row["discount_factor"]))


def test_discounted_payback_matches_the_service():
    import json

    # The cash flows are the JSON body; the two scalars are query params.
    params = {"initial_investment": 100000.0, "discount_rate": 0.1}
    client = TestClient(load_service("discounted-payback-period-service").main.app)
    served = client.post("/calculate", params=params, json=[40000.0, 50000.0, 60000.0])
    assert served.status_code == 200, served.text
    service = served.json()

    rust = json.loads(native.discounted_payback_json(100000.0, [40000.0, 50000.0, 60000.0], 0.1))
    assert Decimal(str(service["discounted_payback_period"])) == Decimal(str(rust["discounted_payback_period"]))
    assert service["payback_achieved_in_year"] == rust["payback_achieved_in_year"]
    assert service["note"] == rust["note"]
    for served_row, rust_row in zip(service["discounted_cash_flow_schedule"], rust["discounted_cash_flow_schedule"]):
        for key in ("discount_factor", "present_value", "cumulative_pv"):
            assert Decimal(str(served_row[key])) == Decimal(str(rust_row[key])), key


def test_arr_matches_the_service():
    params = {
        "initial_investment": 200000.0,
        "residual_value": 20000.0,
        "total_profits": 150000.0,
        "years": 3,
    }
    service, rust = _params_parity("accounting-rate-return-service", "/calculate", params, native.arr_calculate_json)
    for key in (
        "average_annual_profit",
        "average_investment",
        "arr_percentage",
        "arr_decimal",
    ):
        assert Decimal(str(service[key])) == Decimal(str(rust[key])), key


def test_arr_decision_matches_the_service():
    service, rust = _params_parity(
        "accounting-rate-return-service",
        "/decision",
        {"arr": 18.5, "target_return": 12.0},
        native.arr_decision_json,
    )
    assert service["decision"] == rust["decision"]
    assert Decimal(str(service["difference"])) == Decimal(str(rust["difference"]))


# --- hedging and FX family --------------------------------------------------
# Seven small services that were already in treasury_fx but still owned their
# endpoints. Nothing new to port: parity is the gate, so the crate answers only
# once it reproduces the service bodies.

FUTURES_HEDGING_REQUEST = {
    "company_id": "c1",
    "positions": [
        {
            "future_id": "f1",
            "contract_month": "2026-06",
            "position_size": 10.0,
            "entry_price": 100.0,
            "current_price": 112.0,
            "contract_size": 100.0,
        }
    ],
    "spot_exposure": 120000.0,
    "target_hedge_ratio": 0.8,
    "basis_risk": 0.02,
}

COMMODITY_HEDGING_REQUEST = {
    "company_id": "c1",
    "exposures": [
        {
            "commodity": "copper",
            "exposure_type": "long",
            "volume": 50.0,
            "unit": "tonnes",
            "current_price": 8200.0,
            "budget_price": 7800.0,
            "volatility": 0.18,
        }
    ],
    "hedge_ratio": 0.7,
    "budget_rate": {"copper": 7800.0},
}

INTEREST_RATE_HEDGING_REQUEST = {
    "company_id": "c1",
    "exposures": [
        {
            "debt_id": "d1",
            "principal": 500000.0,
            "current_rate": 0.11,
            "rate_type": "variable",
            "maturity_years": 3.0,
            "reference_rate": "SOFR",
        }
    ],
    "total_debt": 500000.0,
    "variable_debt": 500000.0,
    "rate_shock_scenarios": [-0.02, 0.0, 0.02],
    "yield_curve_slope": 0.015,
}

TRANSLATION_REQUEST = {
    "company_id": "c1",
    "subsidiary": "Vimbai ZW",
    "functional_currency": "USD",
    "presentation_currency": "ZWG",
    "closing_rate": 26.5,
    "avg_rate": 25.0,
    "historical_rate": 24.0,
    "net_assets": [
        {"account": "Property", "amount": 400000.0, "is_monetary": False},
        {"account": "Receivables", "amount": 120000.0, "is_monetary": True},
    ],
    "income_statement": [{"account": "Revenue", "amount": 900000.0, "is_monetary": False}],
    "goodwill": 50000.0,
}

SWAP_VALUATION_REQUEST = {
    "company_id": "c1",
    "swaps": [
        {
            "swap_id": "s1",
            "swap_type": "interest_rate",
            "notional": 1000000.0,
            "fixed_rate": 0.08,
            "floating_rate": 0.095,
            "tenor_years": 3,
            "payment_frequency": "annual",
        }
    ],
    "current_yield_curve": {"1Y": 0.09, "3Y": 0.095},
    "ois_rate": 0.085,
}

CROSS_CURRENCY_SWAP_REQUEST = {
    "company_id": "c1",
    "swaps": [
        {
            "swap_id": "x1",
            "notional_1": 1000000.0,
            "currency_1": "USD",
            "rate_1": 0.05,
            "notional_2": 26500000.0,
            "currency_2": "ZWG",
            "rate_2": 0.12,
            "tenor_years": 3.0,
            "exchange_at_inception": True,
        }
    ],
    "fx_spot": {"USD/ZWG": 26.5},
    "ois_rates": {"USD": 0.05, "ZWG": 0.12},
}

CURRENCY_HEDGING_REQUEST = {
    "company_id": "c1",
    "exposures": [
        {
            "currency_pair": "USD/ZWG",
            "exposure_amount": 250000.0,
            "spot_rate": 26.5,
            "expected_rate": 28.0,
            "volatility": 0.14,
        }
    ],
    "current_hedge_ratio": 0.4,
    "target_hedge_ratio": 0.75,
    "risk_aversion": 0.6,
}


def _bodies_match(service, rust, path=""):
    """Recursively compare two JSON bodies, treating floats as decimals."""
    if isinstance(service, dict) and isinstance(rust, dict):
        for key in set(service) | set(rust):
            assert key in service, f"extra key in rust at {path}.{key}"
            assert key in rust, f"missing key in rust at {path}.{key}"
            _bodies_match(service[key], rust[key], f"{path}.{key}")
    elif isinstance(service, list) and isinstance(rust, list):
        assert len(service) == len(rust), f"length differs at {path}"
        for index, (left, right) in enumerate(zip(service, rust)):
            _bodies_match(left, right, f"{path}[{index}]")
    elif (
        isinstance(service, (int, float))
        and isinstance(rust, (int, float))
        and not isinstance(service, bool)
        and not isinstance(rust, bool)
    ):
        assert Decimal(str(service)) == Decimal(str(rust)), f"{path}: {service} vs {rust}"
    else:
        assert service == rust, f"{path}: {service!r} vs {rust!r}"


def _hedging_parity(service_name, path, request, seam_call, drop=()):
    import json

    client = TestClient(load_service(service_name).main.app)
    served = client.post(path, json=request)
    assert served.status_code == 200, served.text
    service = served.json()
    rust = json.loads(seam_call(json.dumps(request)))
    for key in drop:
        service.pop(key, None)
        rust.pop(key, None)
    _bodies_match(service, rust)
    return service, rust


def test_futures_hedging_matches_the_service():
    _hedging_parity(
        "futures-hedging-service",
        "/analyze",
        FUTURES_HEDGING_REQUEST,
        native.fx_futures_hedging_json,
    )


def test_commodity_hedging_matches_the_service():
    _hedging_parity(
        "commodity-hedging-service",
        "/analyze",
        COMMODITY_HEDGING_REQUEST,
        native.fx_commodity_hedging_json,
    )


def test_interest_rate_hedging_matches_the_service():
    _hedging_parity(
        "interest-rate-hedging-service",
        "/analyze",
        INTEREST_RATE_HEDGING_REQUEST,
        native.fx_interest_rate_hedging_json,
    )


def test_fx_translation_matches_the_service():
    # The service mints a uuid per translation; the crate is stateless.
    _hedging_parity(
        "foreign-currency-translation-service",
        "/translate",
        TRANSLATION_REQUEST,
        native.fx_translation_json,
        drop=("id",),
    )


def test_swap_valuation_matches_the_service():
    _hedging_parity(
        "swap-valuation-service",
        "/value",
        SWAP_VALUATION_REQUEST,
        native.fx_swap_valuation_json,
    )


def test_cross_currency_swap_matches_the_service():
    _hedging_parity(
        "cross-currency-swap-service",
        "/value",
        CROSS_CURRENCY_SWAP_REQUEST,
        native.fx_cross_currency_swap_json,
    )


def test_currency_hedging_matches_the_service():
    _hedging_parity(
        "currency-hedging-service",
        "/analyze",
        CURRENCY_HEDGING_REQUEST,
        native.fx_currency_hedging_json,
    )


# --- forwards and multi-currency -------------------------------------------
# forward-contracts is a fresh port; multi-currency was already in the crate and
# only needed the seam. Both are compared whole, minus the ids and timestamps
# the services mint themselves.

FORWARD_CONTRACTS_REQUEST = {
    "company_id": "c1",
    "contracts": [
        {
            "contract_id": "f1",
            "currency_pair": "USD/ZWG",
            "notional": 500000.0,
            "spot_rate": 25.0,
            "forward_rate": 27.0,
            "delivery_date": "2027-05-02",
        },
        {
            "contract_id": "f2",
            "currency_pair": "USD/ZWG",
            "notional": 250000.0,
            "spot_rate": 26.0,
            "forward_rate": 25.5,
            "delivery_date": "2027-05-02",
        },
    ],
    "current_spot_rate": 26.5,
    "risk_free_rate_domestic": 0.12,
    "risk_free_rate_foreign": 0.05,
    "volatility": 0.15,
}

MULTI_CURRENCY_CONVERT_REQUEST = {
    "company_id": "c1",
    "amount": 10000.0,
    "from_currency": "USD",
    "to_currency": "EUR",
    "conversion_date": "2026-11-02",
}

MULTI_CURRENCY_TRANSACTION_REQUEST = {
    "company_id": "c1",
    "transaction_id": "t1",
    "amount": 5000.0,
    "currency": "EUR",
    "transaction_type": "payment",
    "counterparty_currency": "GBP",
}

MULTI_CURRENCY_HEDGE_REQUEST = {
    "company_id": "c1",
    "exposure_amount": 250000.0,
    "currency_pair": "EURUSD",
    "hedge_type": "forward",
    "tenor": 6,
    "notional": 250000.0,
}


def test_forward_contracts_match_the_service():
    _hedging_parity(
        "forward-contracts-service",
        "/analyze",
        FORWARD_CONTRACTS_REQUEST,
        native.fx_forward_contracts_json,
    )


def test_multi_currency_convert_matches_the_service():
    # converted_at is the service's clock read; the seam takes it as an argument.
    _hedging_parity(
        "multi-currency-service",
        "/convert",
        MULTI_CURRENCY_CONVERT_REQUEST,
        lambda payload: native.fx_multi_currency_convert_json(payload, "2026-11-02T00:00:00"),
        drop=("converted_at",),
    )


def test_multi_currency_transaction_matches_the_service():
    _hedging_parity(
        "multi-currency-service",
        "/transaction",
        MULTI_CURRENCY_TRANSACTION_REQUEST,
        native.fx_multi_currency_transaction_json,
    )


def test_multi_currency_hedge_matches_the_service():
    _hedging_parity(
        "multi-currency-service",
        "/hedge",
        MULTI_CURRENCY_HEDGE_REQUEST,
        lambda payload: native.fx_multi_currency_hedge_json(payload, "hedge-1"),
        drop=("hedge_id",),
    )


# --- credit and exotic derivatives -----------------------------------------

CREDIT_DERIVATIVES_REQUEST = {
    "company_id": "c1",
    "positions": [
        {
            "position_id": "p1",
            "instrument_type": "CDS",
            "reference_entity": "Bank A",
            "notional": 5000000.0,
            "spread": 250.0,
            "rating": "BBB",
            "maturity_years": 5.0,
        },
        {
            "position_id": "p2",
            "instrument_type": "CDS_INDEX",
            "reference_entity": "CDX IG",
            "notional": 2000000.0,
            "spread": 180.0,
            "rating": "A",
            "maturity_years": 3.0,
        },
    ],
    "risk_free_rate": 0.05,
    "recovery_rate": 0.4,
    "credit_spreads": {"Bank A": 220.0, "CDX IG": 200.0},
}

EXOTIC_VANILLA = {
    "option_type": "vanilla",
    "underlying": "ZWG/USD",
    "spot_price": 100.0,
    "strike_price": 105.0,
    "volatility": 0.25,
    "risk_free_rate": 0.06,
    "time_to_expiry": 1.0,
    "barrier_price": 0,
    "barrier_type": "",
    "is_call": True,
    "observations": 12,
}


def _exotic(option_type, **overrides):
    import copy

    request = copy.deepcopy(EXOTIC_VANILLA)
    request["option_type"] = option_type
    request.update(overrides)
    return request


def test_credit_derivatives_match_the_service():
    _hedging_parity(
        "credit-derivatives-service",
        "/analyze",
        CREDIT_DERIVATIVES_REQUEST,
        native.fx_credit_derivatives_json,
    )


def _exotic_parity(request):
    import json

    client = TestClient(load_service("exotic-derivatives-service").main.app)
    served = client.post("/price", json=request)
    assert served.status_code == 200, served.text
    service = served.json()
    rust = json.loads(native.fx_exotic_derivatives_json(json.dumps(request)))
    for key in (
        "estimated_price",
        "delta",
        "gamma",
        "vega",
        "theta",
        "rho",
    ):
        assert Decimal(str(service[key])) == Decimal(str(rust[key])), (
            request["option_type"],
            key,
            service[key],
            rust[key],
        )
    assert service["model_used"] == rust["model_used"]
    assert service["notes"] == rust["notes"]


def test_exotic_vanilla_matches_the_service():
    _exotic_parity(_exotic("vanilla"))


def test_exotic_asian_matches_the_service():
    _exotic_parity(_exotic("asian"))


def test_exotic_lookback_matches_the_service():
    _exotic_parity(_exotic("lookback"))


def test_exotic_binary_matches_the_service():
    _exotic_parity(_exotic("binary"))


def test_exotic_barrier_matches_the_service():
    _exotic_parity(_exotic("barrier", barrier_price=110.0, barrier_type="knock_out"))
    _exotic_parity(_exotic("barrier", barrier_price=90.0, barrier_type="knock_in"))


def test_exotic_put_matches_the_service():
    _exotic_parity(_exotic("vanilla", is_call=False))


# --- liquidity family -------------------------------------------------------

LIQUIDITY_FORECAST_REQUEST = {
    "company_id": "c1",
    "starting_cash": 250000.0,
    "cash_flows": [
        {"date": "2026-11-05", "inflow": 90000.0, "outflow": 40000.0, "net": 50000.0},
        {"date": "2026-11-12", "inflow": 30000.0, "outflow": 120000.0, "net": -90000.0},
        {"date": "2026-11-19", "inflow": 60000.0, "outflow": 55000.0, "net": 5000.0},
    ],
    "credit_lines_available": 100000.0,
    "minimum_cash_requirement": 200000.0,
}

LIQUIDITY_MANAGEMENT_REQUEST = {
    "company_id": "c1",
    "cash": 120000.0,
    "marketable_securities": 80000.0,
    "credit_facilities_available": 150000.0,
    "credit_facilities_total": 300000.0,
    "current_assets": 600000.0,
    "current_liabilities": 350000.0,
    "monthly_fixed_costs": 180000.0,
    "monthly_variable_costs": 90000.0,
    "monthly_min_revenue": 150000.0,
}

LIQUIDITY_RISK_REQUEST = {
    "company_id": "c1",
    "reporting_date": "2026-11-02",
    "liquid_assets": [
        {"asset_id": "a1", "asset_type": "cash", "value": 90000.0, "haircut": 0.0, "maturity_bucket": "0-1m"},
        {"asset_id": "a2", "asset_type": "receivables", "value": 70000.0, "haircut": 0.15, "maturity_bucket": "1-3m"},
    ],
    "liabilities": [
        {
            "liability_id": "l1",
            "liability_type": "payables",
            "value": 120000.0,
            "outflow_rate": 0.6,
            "maturity_bucket": "0-1m",
        },
        {
            "liability_id": "l2",
            "liability_type": "debt",
            "value": 80000.0,
            "outflow_rate": 0.25,
            "maturity_bucket": "1-3m",
        },
    ],
    "cash_flows": {"0-1m": [-20000.0, -15000.0], "1-3m": [-30000.0, 10000.0]},
}


def _liquidity_parity(service_dir, path, seam_name, request):
    client = TestClient(load_service(service_dir).main.app)
    served = client.post(path, json=request)
    assert served.status_code == 200, served.text
    rust = __import__("json").loads(getattr(native, seam_name)(__import__("json").dumps(request)))
    _bodies_match(served.json(), rust, service_dir)
    return served.json(), rust


def test_liquidity_forecast_matches_the_service():
    served, rust = _liquidity_parity(
        "liquidity-forecast-service", "/forecast", "liquidity_forecast_json", LIQUIDITY_FORECAST_REQUEST
    )
    assert len(served["daily_forecast"]) == len(rust["daily_forecast"]) == 3
    assert rust["forecast_summary"]["min_balance"] == served["forecast_summary"]["min_balance"]


def test_liquidity_management_matches_the_service():
    served, rust = _liquidity_parity(
        "liquidity-management-service", "/analyze", "liquidity_management_json", LIQUIDITY_MANAGEMENT_REQUEST
    )
    assert rust["liquidity_ratios"] == served["liquidity_ratios"]
    assert rust["coverage_days"] == served["coverage_days"]


def test_liquidity_risk_matches_the_service():
    served, rust = _liquidity_parity(
        "liquidity-risk-service", "/analyze", "liquidity_risk_json", LIQUIDITY_RISK_REQUEST
    )
    assert rust["liquidity_risk_rating"] == served["liquidity_risk_rating"]
    assert rust["lcr_ratio"] == served["lcr_ratio"]


CASH_FLOW_FORECAST_REQUEST = {
    "company_id": "c1",
    "starting_cash": 100000.0,
    "inflows": [
        {"category": "sales", "amount": 25000.0, "frequency": "daily", "certainty": "high"},
        {"category": "retainer", "amount": 20000.0, "frequency": "weekly", "certainty": "medium"},
        {"category": "grant", "amount": 50000.0, "frequency": "monthly", "certainty": "low"},
    ],
    "outflows": [
        {"category": "payroll", "amount": 29000.0, "frequency": "daily", "certainty": "high"},
        {"category": "rent", "amount": 50000.0, "frequency": "monthly", "certainty": "high"},
    ],
    "forecast_period_days": 45,
    "scenario": "base",
}


def test_cash_flow_forecast_matches_the_service():
    request = CASH_FLOW_FORECAST_REQUEST
    client = TestClient(load_service("cash-flow-forecasting-service").main.app)
    served = client.post("/forecast", json=request)
    assert served.status_code == 200, served.text
    served = served.json()
    # forecast_date is the service's clock reading, so it is passed in.
    rust = __import__("json").loads(
        native.cash_flow_forecast_json(__import__("json").dumps(request), served["forecast_date"])
    )
    _bodies_match(served, rust)
    # monthly items are ignored, so every day carries the same daily figures
    outflows = {day["outflow"] for day in rust["daily_forecasts"]}
    assert outflows == {29000.0}
    # 45 days is 7 weekly buckets, one of them partial
    assert len(rust["weekly_summary"]) == 7
    assert len(rust["monthly_summary"]) == 2


# --- working capital family --------------------------------------------------

WC_ANALYSIS_REQUEST = {
    "company_id": "c1",
    "current_assets": 420000.0,
    "current_liabilities": 260000.0,
    "inventory": 90000.0,
    "accounts_receivable": 120000.0,
    "cash": 60000.0,
    "accounts_payable": 85000.0,
    "short_term_debt": 70000.0,
    "cost_of_goods_sold": 720000.0,
    "revenue": 1200000.0,
}

WC_OPTIMIZE_REQUEST = {
    "company_id": "c1",
    "reporting_date": "2026-11-02",
    "accounts_receivable": 120000.0,
    "accounts_payable": 85000.0,
    "inventory": 90000.0,
    "revenue": 1200000.0,
    "cogs": 720000.0,
    "purchases": 700000.0,
    "days_analysis": 90,
}

WC_FINANCE_REQUEST = {
    "company_id": "c1",
    "period": "FY2026",
    "current_assets": 420000.0,
    "current_liabilities": 260000.0,
    "inventory": 90000.0,
    "accounts_receivable": 120000.0,
    "accounts_payable": 85000.0,
    "annual_revenue": 1200000.0,
    "cogs": 720000.0,
}

FACTORING_REQUEST = {
    "company_id": "c1",
    "invoice_amount": 250000.0,
    "advance_rate": 0.8,
    "factor_fee_rate": 0.02,
    "discount_rate": 0.12,
}


def _wc_parity(service_dir, path, seam_name, request):
    client = TestClient(load_service(service_dir).main.app)
    served = client.post(path, json=request)
    assert served.status_code == 200, served.text
    rust = __import__("json").loads(getattr(native, seam_name)(__import__("json").dumps(request)))
    _bodies_match(served.json(), rust)
    return served.json(), rust


def test_working_capital_analysis_matches_the_service():
    served, rust = _wc_parity("working-capital-service", "/analyze", "wc_analyze_json", WC_ANALYSIS_REQUEST)
    # the cycle is DSO + DIO - DPO, so a long payable tail shortens it
    assert rust["cash_conversion_cycle"] == served["cash_conversion_cycle"]


def test_working_capital_optimization_matches_the_service():
    served, rust = _wc_parity(
        "working-capital-optimization-service",
        "/optimize",
        "wc_optimize_json",
        WC_OPTIMIZE_REQUEST,
    )
    assert rust["cash_conversion_cycle"] == served["cash_conversion_cycle"]


def _wc_parity_without_id(path, seam_name, request):
    """The finance endpoints mint a uuid per report, so that key is skipped."""
    client = TestClient(load_service("working-capital-finance-service").main.app)
    served = client.post(path, json=request)
    assert served.status_code == 200, served.text
    served = served.json()
    rust = __import__("json").loads(getattr(native, seam_name)(__import__("json").dumps(request)))
    served.pop("id", None)
    _bodies_match(served, rust)
    return served, rust


def test_working_capital_finance_matches_the_service():
    _wc_parity_without_id("/analyze", "wc_finance_json", WC_FINANCE_REQUEST)


def test_working_capital_factoring_matches_the_service():
    _wc_parity_without_id("/factoring", "wc_factoring_json", FACTORING_REQUEST)
