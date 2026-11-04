"""Parity for services whose calculation already lived in Rust but was never routed.

The payloads are built from each service's own request model, so the comparison covers
the fields the service actually publishes rather than a hand-written guess. Every case
is checked against the service's response, which is the reference.
"""

import enum
import inspect
import json
import typing

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service

native = pytest.importorskip("vimbai_native", reason="build library/native with maturin to test the seam")

STRINGS = {
    "company_id": "co-1",
    "parent_company_id": "parent-1",
    "parent_id": "parent-1",
    "period": "2026",
    "fiscal_year": "2026",
    "currency": "USD",
    "reporting_currency": "USD",
    "local_currency": "USD",
    "jurisdiction": "ZW",
    "description": "Inventory write-down",
    "name": "Harare",
}

STAMPS = {
    "id",
    "assessment_date",
    "assessment_id",
    "created_at",
    "updated_at",
    "generated_at",
    "submission_date",
}

# Declared on the pension result model and never set by anything, so it is a constant
# zero rather than a figure either side can disagree about.
PLACEHOLDERS = {"oci_return_adjustment"}

CASES = [
    ("regulatory-ratio-service", "/calculate", "regulatory_ratio_json"),
    ("tax-risk-service", "/assess", "tax_risk_json"),
    ("group-tax-service", "/consolidate", "group_tax_json"),
    ("vat-reporting-service", "/prepare-return", "vat_return_json"),
    ("government-grants-service", "/recognize", "grant_json"),
    ("balanced-scorecard-service", "/score", "scorecard_json"),
    ("financial-distress-prediction-service", "/predict", "distress_json"),
    ("fraud-risk-assessment-service", "/assess", "fraud_risk_json"),
]


def _value(annotation, field: str, depth: int = 0):
    origin = typing.get_origin(annotation)
    if origin is typing.Literal:
        choices = typing.get_args(annotation)
        return choices[0] if choices else None
    if origin is typing.Union:
        args = [a for a in typing.get_args(annotation) if a is not type(None)]
        return _value(args[0], field, depth) if args else None
    if origin in (list, typing.List):
        args = typing.get_args(annotation)
        inner = args[0] if args else str
        return [_value(inner, field, depth + 1) for _ in range(1)]
    if origin in (dict, typing.Dict):
        return {}
    if inspect.isclass(annotation) and hasattr(annotation, "model_fields"):
        return _payload_for(annotation, depth + 1)
    if inspect.isclass(annotation) and issubclass(annotation, enum.Enum):
        return list(annotation)[0].value
    name = getattr(annotation, "__name__", str(annotation)).lower()
    if name.startswith("literal"):
        return None
    if annotation is str or name == "str":
        return STRINGS.get(field, "sample")
    if annotation is int or name == "int":
        return 2026 if "year" in field else 3
    if annotation is bool or name == "bool":
        return True
    if field in ("probability", "volatility", "rate", "tax_rate", "growth_rate"):
        return 0.5
    if annotation is float or name == "float":
        return 1_000_000.0
    # Anything the model leaves open-ended is filled as text: a number where a string
    # is expected fails validation, and the endpoint would never be reached.
    return "sample"


def _payload_for(model, depth: int = 0):
    payload = {}
    for field, info in getattr(model, "model_fields", {}).items():
        payload[field] = _value(info.annotation, field, depth)
    return payload


def _request_payload(route):
    signature = inspect.signature(route)
    for param in signature.parameters.values():
        annotation = param.annotation
        if inspect.isclass(annotation) and hasattr(annotation, "model_fields"):
            return _payload_for(annotation)
    return {}


@pytest.mark.parametrize("service,path,seam", CASES)
def test_served_calculation_matches_the_service(service, path, seam):
    module = load_service(service).main
    route = next(
        r.endpoint
        for r in module.app.routes
        if getattr(r, "path", None) == path and "POST" in getattr(r, "methods", set())
    )
    payload = _request_payload(route)
    served = TestClient(module.app).post(path, json=payload)
    assert served.status_code == 200, (service, payload, served.text)
    rust = json.loads(getattr(native, seam)(json.dumps(payload)))
    # The service stamps an identifier and a timestamp on some answers. They are not
    # figures, they differ on every call, and the Rust side has no business inventing
    # them, so they are set aside rather than compared.
    published = {key: value for key, value in served.json().items() if key not in STAMPS}
    bare = {key: value for key, value in rust.items() if key not in STAMPS}
    assert published == bare, service


# The four remaining routes publish their own requester shapes, so their payloads are
# written out rather than derived: a nested partner list built from type hints alone
# omits the partner identity the library needs.

PARTNERSHIP = {
    "pension-accounting-service": (
        "/calculate",
        "pension_json",
        {
            "company_id": "co-1",
            "fiscal_year": 2026,
            "plan_name": "Main scheme",
            "obligation_beginning": 4_200_000.0,
            "service_cost": 320_000.0,
            "interest_cost": 210_000.0,
            "benefits_paid": 180_000.0,
            "actuarial_gain_loss": 45_000.0,
            "past_service_cost": 12_500.0,
            "plan_assets_beginning": 3_600_000.0,
            "contributions": 260_000.0,
            "expected_return": 252_000.0,
            "actual_return": 268_400.0,
            "benefits_paid_from_assets": 180_000.0,
        },
    ),
    "partnership-accounting-service": (
        "/allocate",
        "partnership_allocation_json",
        {
            "company_id": "co-1",
            "period": "2026",
            "net_profit": 1_500_000.0,
            "interest_rate_on_capital": 0.05,
            "partners": [
                {
                    "partner_id": "p-1",
                    "name": "Tendai",
                    "capital_contribution": 800_000.0,
                    "salary": 240_000.0,
                    "profit_share_pct": 60.0,
                    "interest_on_capital_rate": 0.05,
                    "drawings": 40_000.0,
                },
                {
                    "partner_id": "p-2",
                    "name": "Rudo",
                    "capital_contribution": 400_000.0,
                    "salary": 180_000.0,
                    "profit_share_pct": 40.0,
                    "interest_on_capital_rate": 0.05,
                    "drawings": 15_000.0,
                },
            ],
        },
    ),
}

PARTNERSHIP_SALE = {
    ("partnership-sale-service", "/goodwill", "partnership_goodwill_json"): {
        "company_id": "co-1",
        "avg_profit": 900_000.0,
        "years": 3,
        "rate": 0.15,
        "partners": [
            {"partner_id": "p-1", "name": "Tendai", "capital": 800_000.0, "share_pct": 60.0},
            {"partner_id": "p-2", "name": "Rudo", "capital": 400_000.0, "share_pct": 40.0},
        ],
    },
    ("partnership-sale-service", "/retirement", "partner_buyout_json"): {
        "company_id": "co-1",
        "outgoing_partner_id": "p-1",
        "outgoing_partner_name": "Tendai",
        "capital_balance": 720_000.0,
        "share_of_goodwill": 180_000.0,
        "share_of_reserves": 90_000.0,
        "loan_to_partner": 25_000.0,
        "agreed_payment": 965_000.0,
        "remaining_partners": [{"partner_id": "p-2", "name": "Rudo", "share_pct": 100.0}],
    },
}


@pytest.mark.parametrize("service", sorted(PARTNERSHIP))
def test_pension_and_partnership_shapes_match_the_service(service):
    path, seam, payload = PARTNERSHIP[service]
    module = load_service(service).main
    served = TestClient(module.app).post(path, json=payload)
    assert served.status_code == 200, (service, served.text)
    rust = json.loads(getattr(native, seam)(json.dumps(payload)))
    published = {key: value for key, value in served.json().items() if key not in STAMPS and key not in PLACEHOLDERS}
    assert published == rust, service


@pytest.mark.parametrize("case", sorted(PARTNERSHIP_SALE))
def test_partnership_sale_shapes_match_the_service(case):
    service, path, seam = case
    payload = PARTNERSHIP_SALE[case]
    module = load_service(service).main
    served = TestClient(module.app).post(path, json=payload)
    assert served.status_code == 200, (service, path, served.text)
    rust = json.loads(getattr(native, seam)(json.dumps(payload)))
    published = {key: value for key, value in served.json().items() if key not in STAMPS}
    assert published == rust, (service, path)
