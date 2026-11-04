"""Parity between the cost-of-capital seam and its service.

The Python service stays in the tree as the reference. These tests hold the service
and the Rust port together answer-for-answer, including the refusals, because the
service answers those with a 200 and an `error` body and a client may depend on it.
"""

import json

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service

native = pytest.importorskip("vimbai_native", reason="build library/native with maturin to test the seam")


def _seam(name, payload):
    return json.loads(getattr(native, name)(json.dumps(payload)))


@pytest.fixture(scope="module")
def client():
    return TestClient(load_service("cost-of-capital-service").main.app)


CASES = [
    ("/calculate", "cost_of_capital_calculate_json", {"interest_rate": 0.08, "risk_premium": 0.03}),
    ("/calculate", "cost_of_capital_calculate_json", {"interest_rate": 0.1, "risk_premium": 0}),
    (
        "/from-percent",
        "cost_of_capital_from_percent_json",
        {"interest_rate_percent": 8, "risk_premium_percent": 3},
    ),
    (
        "/wacc",
        "cost_of_capital_wacc_json",
        {
            "equity_value": 600000,
            "cost_of_equity": 0.12,
            "debt_value": 400000,
            "cost_of_debt": 0.08,
            "tax_rate": 25,
        },
    ),
    (
        "/wacc",
        "cost_of_capital_wacc_json",
        {"equity_value": 250000, "cost_of_equity": 0.15, "debt_value": 750000, "cost_of_debt": 0.07},
    ),
    (
        "/wacc",
        "cost_of_capital_wacc_json",
        {"equity_value": 0, "cost_of_equity": 0.1, "debt_value": 0, "cost_of_debt": 0.1},
    ),
    (
        "/simple-rate",
        "cost_of_capital_simple_rate_json",
        {"principal": 1000, "amount_after": 1150, "years": 3},
    ),
    (
        "/compound-rate",
        "cost_of_capital_compound_rate_json",
        {"present_value": 1000, "future_value": 1610.51, "years": 5},
    ),
    (
        "/compound-rate",
        "cost_of_capital_compound_rate_json",
        {"present_value": 0, "future_value": 5000, "years": 10},
    ),
]


@pytest.mark.parametrize("path, seam_name, params", CASES)
def test_cost_of_capital_matches_the_service(client, path, seam_name, params):
    served = client.post(path, params=params)
    assert served.status_code == 200, served.text
    assert served.json() == _seam(seam_name, params)


def test_the_standard_rate_table_matches_the_service(client):
    served = client.get("/list")
    assert served.status_code == 200
    assert served.json() == _seam("cost_of_capital_list_json", {})
