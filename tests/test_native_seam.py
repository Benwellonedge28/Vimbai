"""Parity between the Rust calculation seam and the Python services.

The seam only earns trust if it reproduces what the services already report,
so these tests run the same request through both and compare the figures
instead of checking that a call succeeded.
"""

import os
import sys
from decimal import Decimal

import pytest

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
