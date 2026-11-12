"""Parity between cost-accounting-service and the library's cost core.

Job costing and the `/standards` variance view are pure calculations behind this
service, so they can be held to Rust without touching its storage. The service rounds
and reshapes both answers, and those shapes are what a client sees, so the comparison
is on the served JSON rather than the raw figures.
"""

import json

import pytest
from fastapi.testclient import TestClient

from tests.conftest import load_service

native = pytest.importorskip("vimbai_native", reason="build rust/native with maturin to test the seam")


def _seam(name, payload):
    return json.loads(getattr(native, name)(json.dumps(payload)))


def _figures(body):
    """Everything but the per-response uuid the service's response model adds."""
    return {key: value for key, value in body.items() if key != "id"}


@pytest.fixture(scope="module")
def client():
    return TestClient(load_service("cost-accounting-service").main.app)


JOBS = [
    {
        "company_id": "c-1",
        "job_id": "j-1",
        "job_name": "Batch 7",
        "direct_materials": 4200.555,
        "direct_labour": 3100.25,
        "direct_labour_hours": 155.5,
        "overhead_rate": 12.4,
        "units_produced": 240,
        "additional_costs": [
            {"description": "Setup", "amount": 180.4},
            {"description": "Finishing", "amount": 96.35},
        ],
    },
    {
        "company_id": "c-1",
        "job_id": "j-2",
        "job_name": "No units stated",
        "direct_materials": 500.0,
        "direct_labour": 250.0,
        "direct_labour_hours": 10.0,
        "overhead_rate": 9.0,
        "units_produced": 0,
        "additional_costs": [],
    },
]

STANDARDS = [
    {
        "company_id": "c-1",
        "product_name": "Widget",
        "direct_materials_std": 12.5,
        "direct_labor_std": 8.0,
        "overhead_std": 4.5,
        "units_produced": 1000,
        "actual_materials": 12900.0,
        "actual_labor": 7950.0,
        "actual_overhead": 4700.0,
    },
    {
        "company_id": "c-1",
        "product_name": "Gadget",
        "direct_materials_std": 6.0,
        "direct_labor_std": 3.5,
        "overhead_std": 2.0,
        "units_produced": 400,
        "actual_materials": 2400.0,
        "actual_labor": 1500.0,
        "actual_overhead": 850.0,
    },
    {
        "company_id": "c-1",
        "product_name": "No output",
        "direct_materials_std": 6.0,
        "direct_labor_std": 3.5,
        "overhead_std": 2.0,
        "units_produced": 0,
        "actual_materials": 100.0,
        "actual_labor": 50.0,
        "actual_overhead": 20.0,
    },
]


@pytest.mark.parametrize("payload", JOBS)
def test_job_cost_matches_the_service(client, payload):
    served = client.post("/job-cost", json=payload)
    assert served.status_code == 200, served.text
    assert _figures(served.json()) == _seam("cost_accounting_job_cost_json", payload)


@pytest.mark.parametrize("payload", STANDARDS)
def test_standard_costing_matches_the_service(client, payload):
    served = client.post("/standards", json=payload)
    assert served.status_code == 200, served.text
    assert _figures(served.json()) == _seam("cost_accounting_standards_json", payload)
