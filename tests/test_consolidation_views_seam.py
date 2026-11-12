"""Parity for the consolidation service's breakdown endpoints.

The consolidated statement itself is covered elsewhere; these are the views published
beside it. Each carries a fixed assumption the service made (a standard profit and
equity for NCI, a flat net-asset figure for goodwill, one rate for every balance), so
the comparison is on the published JSON rather than on the inputs.
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
    return TestClient(load_service("consolidation-service").main.app)


SUBSIDIARIES = [
    {
        "subsidiary_id": "sub-1",
        "subsidiary_name": "Harare Holdings",
        "ownership_percentage": 75.0,
        "non_controlling_interests_percentage": 25.0,
        "financial_data": {"profit": 640_000.0, "equity": 3_100_000.0},
    },
    {
        "subsidiary_id": "sub-2",
        "subsidiary_name": "Bulawayo Trading",
        "ownership_percentage": 60.0,
        "non_controlling_interests_percentage": 40.0,
    },
    {
        "subsidiary_id": "sub-3",
        "subsidiary_name": "Mutare Minor",
        "ownership_percentage": 40.0,
        "non_controlling_interests_percentage": 0.0,
        "financial_data": {"profit": 125_500.55, "equity": 780_000.25},
    },
]

CONSOLIDATION = {
    "parent_company_id": "parent-9",
    "period": "2026-Q3",
    "subsidiaries": SUBSIDIARIES,
    "intercompany_transactions": [
        {"transaction_id": "t-1", "transaction_type": "sale", "amount": 125_400.75},
        {"transaction_id": "t-2", "transaction_type": "purchase", "amount": 82_100.10},
        {"transaction_id": "t-3", "transaction_type": "receivable", "amount": 45_000.00},
        {"transaction_id": "t-4", "transaction_type": "payable", "amount": 12_500.45},
        {"transaction_id": "t-5", "transaction_type": "dividend", "amount": 30_000.00},
        {"transaction_id": "t-6", "transaction_type": "loan", "amount": 250_000.00},
        {"transaction_id": "t-7", "transaction_type": "management_fee", "amount": 18_750.25},
        {
            "transaction_id": "t-8",
            "transaction_type": "rebate",
            "amount": 999.99,
            "parent_party": "parent-9",
            "subsidiary_party": "sub-1",
            "currency": "USD",
            "transaction_date": "2026-08-14",
        },
    ],
}


@pytest.mark.parametrize(
    "path,seam",
    [
        ("/eliminations", "consolidation_eliminations_json"),
        ("/nci-calculation", "consolidation_nci_json"),
        ("/goodwill-calculation", "consolidation_goodwill_json"),
    ],
)
def test_consolidation_views_match_the_service(client, path, seam):
    served = client.post(path, json=CONSOLIDATION)
    assert served.status_code == 200, served.text
    assert served.json() == _seam(seam, CONSOLIDATION)


def test_currency_translation_matches_the_service(client):
    payload = {
        "parent_id": "parent-9",
        "local_currency": "ZWL",
        "reporting_currency": "USD",
        "exchange_rates": {"ZWL": 0.00042, "USD": 1.0},
        "balances": {
            "cash": 8_500_000.0,
            "receivables": 3_250_000.5,
            "shareholders_equity": 12_000_000.0,
            "retained_earnings": 900_000.25,
            "payables": 1_100_000.0,
        },
    }
    served = client.post("/currency-translation", json=payload)
    assert served.status_code == 200, served.text
    assert served.json() == _seam("consolidation_currency_translation_json", payload)


@pytest.mark.parametrize("rules", [[], ["trial_balance"], ["trial_balance", "fx_rates_loaded"]])
def test_validation_matches_the_service(client, rules):
    payload = {
        "parent_id": "parent-9",
        "period": "2026-Q3",
        "entities": ["parent-9", "sub-1", "sub-2", "sub-3"],
        "validation_rules": rules,
    }
    served = client.post("/validate", json=payload)
    assert served.status_code == 200, served.text
    assert served.json() == _seam("consolidation_validate_json", payload)
