"""
Substantive Testing Service
Port: 8198
Detailed testing of account balances and transactions
"""

import importlib.util
import os
import sys
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("substantive_testing_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["substantive_testing_calc"] = calc
_spec.loader.exec_module(calc)

logger = structlog.get_logger()
app = FastAPI(title="Substantive Testing Service", version="1.0.0")


class TestItem(BaseModel):
    item_id: str
    description: str
    amount: float
    testing_type: str
    result: str
    misstatement: float


class SubstantiveTestRequest(BaseModel):
    audit_id: str
    account_id: str
    assertions: List[str]
    population: List[Dict[str, Any]]
    sample_size: int
    misstatement_threshold: float


class SubstantiveTestResponse(BaseModel):
    audit_id: str
    account_id: str
    tests_performed: List[str]
    items_tested: int
    exceptions_found: int
    total_misstatement: float
    projected_misstatement: float
    conclusion: str
    recommendation: str


async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "substantive-testing", "version": "1.0.0"}


def _reference(payload):
    """The original Python calculation, kept as the oracle."""
    exceptions = []
    total_misstatement = 0.0

    for item in payload["population"][: payload["sample_size"]]:
        misstatement = item.get("misstatement", 0.0)
        total_misstatement += abs(misstatement)
        if abs(misstatement) > payload["misstatement_threshold"]:
            exceptions.append(
                {"item_id": item.get("item_id", ""), "amount": item.get("amount", 0.0), "misstatement": misstatement}
            )

    population_total = sum(item.get("amount", 0.0) for item in payload["population"])
    sample_ratio = len(payload["population"]) / payload["sample_size"] if payload["sample_size"] else 1
    projected = total_misstatement * sample_ratio

    conclusion = "unqualified" if projected < payload["misstatement_threshold"] * 0.1 else "qualified"

    return dict(
        audit_id=payload["audit_id"],
        account_id=payload["account_id"],
        tests_performed=[
            "existence",
            "completeness",
            "valuation",
            "rights_and_obligations",
            "accuracy",
            "cutoff",
            "classification",
            "presentation",
        ],
        items_tested=payload["sample_size"],
        exceptions_found=len(exceptions),
        total_misstatement=round(total_misstatement, 2),
        projected_misstatement=round(projected, 2),
        conclusion=conclusion,
        recommendation="Extend testing" if len(exceptions) > 3 else "Conclude on account balance",
    )


@app.post("/test", response_model=SubstantiveTestResponse)
async def perform_substantive_tests(request: SubstantiveTestRequest):
    logger.info("Testing a sample", audit=request.audit_id)
    result = calc.test_sample(request.model_dump(), _reference)
    return SubstantiveTestResponse(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8198)
