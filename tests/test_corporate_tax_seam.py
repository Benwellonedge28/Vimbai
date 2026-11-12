"""Parity for corporate-tax-service.

Both endpoints are pure calculation. `/calculate` keeps the service's adjustment rule,
where only the word "addition" adds and anything else subtracts, so a caller sending
"add" gets a deduction and always has. `/compute` is the flat-rate view kept for older
clients.
"""

import json

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service

native = pytest.importorskip("vimbai_native", reason="build rust/native with maturin to test the seam")


def _seam(name, payload):
    return json.loads(getattr(native, name)(json.dumps(payload)))


@pytest.fixture(scope="module")
def client():
    return TestClient(load_service("corporate-tax-service").main.app)


def _body(**overrides):
    body = {
        "company_id": "co-1",
        "fiscal_year": 2026,
        "accounting_profit": 1_250_000.0,
        "statutory_rate": 0.25,
        "adjustments": [
            {"type": "addition", "description": "Disallowed entertainment", "amount": 45_000.0},
            {"type": "add", "description": "Misspelled addition", "amount": 12_500.0},
            {"type": "deduction", "description": "Exempt dividend", "amount": 30_000.0},
        ],
        "credits": [{"description": "Foreign tax", "amount": 15_000.0}],
        "estimated_payments": 40_000.0,
    }
    body.update(overrides)
    return body


@pytest.mark.parametrize(
    "body",
    [
        _body(),
        _body(adjustments=[]),
        _body(accounting_profit=-80_000.0),
        _body(statutory_rate=0.0, credits=[], estimated_payments=0.0),
    ],
)
def test_calculate_matches_the_service(client, body):
    served = client.post("/calculate", json=body)
    assert served.status_code == 200, served.text
    figures = {k: v for k, v in served.json().items() if k != "id"}
    assert figures == _seam("corporate_tax_json", body)


@pytest.mark.parametrize(
    "body",
    [
        {
            "company_id": "co-1",
            "tax_year": 2026,
            "revenue": 2_000_000.0,
            "deductible_expenses": 1_400_000.0,
            "capital_allowances": 150_000.0,
            "tax_rate": 25.0,
            "credits": 20_000.0,
        },
        {
            "company_id": "co-1",
            "tax_year": 2026,
            "revenue": 500_000.0,
            "deductible_expenses": 620_000.0,
            "capital_allowances": 0.0,
        },
        {
            "company_id": "co-1",
            "tax_year": 2026,
            "revenue": 0.0,
            "deductible_expenses": 0.0,
        },
    ],
)
def test_compute_matches_the_service(client, body):
    served = client.post("/compute", json=body)
    assert served.status_code == 200, served.text
    assert served.json() == _seam("corporate_tax_compute_json", body)
