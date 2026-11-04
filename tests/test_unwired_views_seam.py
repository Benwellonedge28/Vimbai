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
