"""
Activity-Based Costing Service
Port: 8179
ABC cost pools, activity drivers, unit-level/batch-level costs
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "activity_based_costing_service" not in _sys.modules or not hasattr(
    _sys.modules.get("activity_based_costing_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "activity_based_costing_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["activity_based_costing_service"] = _pkg
    _sys.modules["activity_based_costing_service"].__path__ = [_HERE]

from typing import Any, Dict, List

import httpx
import structlog
from activity_based_costing_service import calc
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Activity-Based Costing Service", version="1.0.0")


class ABCActivity(BaseModel):
    activity_id: str
    activity_name: str
    activity_type: str
    cost_pool: float
    cost_driver: str
    driver_volume: int


class ABCProduct(BaseModel):
    product_id: str
    product_name: str
    unit_level_drivers: Dict[str, int]
    batch_level_drivers: Dict[str, int]
    product_level_activities: Dict[str, float]


class ABCRequest(BaseModel):
    company_id: str
    activities: List[ABCActivity]
    products: List[ABCProduct]


class ABCResponse(BaseModel):
    company_id: str
    activity_rates: List[Dict[str, Any]]
    product_costs: List[Dict[str, Any]]
    unit_costs: Dict[str, float]
    total_overhead_allocated: float


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
    return {
        "status": "healthy",
        "service": "activity-based-costing",
        "version": "1.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/calculate", response_model=ABCResponse)
async def calculate_abc(request: ABCRequest):
    logger.info("Calculating ABC", company=request.company_id)

    return ABCResponse(
        **calc.calculate(
            request.company_id,
            [a.model_dump() for a in request.activities],
            [p.model_dump() for p in request.products],
        )
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8179)
