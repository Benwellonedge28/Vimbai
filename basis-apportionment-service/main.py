"""
Vimbai Basis of Apportionment Service
Handles different basis for apportioning costs to cost centres.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "basis-apportionment-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8070"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Basis of Apportionment Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class ApportionmentBasis(str):
    FLOOR_AREA = "floor_area"
    VOLUME = "volume"
    COST = "cost"
    NET_BOOK_VALUE = "net_book_value"
    REPLACEMENT_VALUE = "replacement_value"
    NUMBER_OF_REQUISITIONS = "number_of_requisitions"
    VALUE_OF_REQUISITIONS = "value_of_requisitions"
    NUMBER_OF_PERSONNEL = "number_of_personnel"
    MACHINE_HOURS = "machine_hours"
    LABOR_HOURS = "labor_hours"
    UNITS = "units"


class CostCentreData(BaseModel):
    cost_centre_id: str
    cost_centre_name: str
    basis_value: float  # The value used for apportionment


class ApportionmentRule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_type: str  # rent, rates, depreciation, insurance, maintenance, etc.
    basis: str  # floor_area, volume, cost, etc.
    description: str
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ApportionmentResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_type: str
    total_cost: float
    basis: str
    cost_centres: List[Dict[str, Any]] = []
    total_apportioned: float = 0
    unapportioned: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


apportionment_rules: List[ApportionmentRule] = []
apportionment_results: List[ApportionmentResult] = []


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception:
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Basis of apportionment"}


@app.post("/rules/create")
async def create_apportionment_rule(cost_type: str, basis: str, description: str):
    """Create an apportionment rule."""
    rule = ApportionmentRule(cost_type=cost_type, basis=basis, description=description)
    apportionment_rules.append(rule)
    return rule


@app.get("/rules")
async def list_apportionment_rules():
    """List all apportionment rules."""
    return {"rules": apportionment_rules}


@app.post("/apportion/floor-area")
async def apportion_by_floor_area(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, floor_area}]
):
    """Apportion costs by floor area."""
    result = ApportionmentResult(cost_type=cost_type, total_cost=total_cost, basis="floor_area")

    total_floor_area = sum(c.get("floor_area", 0) for c in cost_centre_data)

    for centre in cost_centre_data:
        floor_area = centre.get("floor_area", 0)
        if total_floor_area > 0:
            proportion = floor_area / total_floor_area
            apportioned_cost = total_cost * proportion
            result.cost_centres.append(
                {
                    "cost_centre_id": centre["cost_centre_id"],
                    "cost_centre_name": centre["cost_centre_name"],
                    "floor_area": floor_area,
                    "proportion": proportion,
                    "apportioned_cost": apportioned_cost,
                }
            )
            result.total_apportioned += apportioned_cost

    result.unapportioned = total_cost - result.total_apportioned
    apportionment_results.append(result)
    return result


@app.post("/apportion/volume")
async def apportion_by_volume(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, volume}]
):
    """Apportion costs by volume (space occupied)."""
    result = ApportionmentResult(cost_type=cost_type, total_cost=total_cost, basis="volume")

    total_volume = sum(c.get("volume", 0) for c in cost_centre_data)

    for centre in cost_centre_data:
        volume = centre.get("volume", 0)
        if total_volume > 0:
            proportion = volume / total_volume
            apportioned_cost = total_cost * proportion
            result.cost_centres.append(
                {
                    "cost_centre_id": centre["cost_centre_id"],
                    "cost_centre_name": centre["cost_centre_name"],
                    "volume": volume,
                    "proportion": proportion,
                    "apportioned_cost": apportioned_cost,
                }
            )
            result.total_apportioned += apportioned_cost

    result.unapportioned = total_cost - result.total_apportioned
    apportionment_results.append(result)
    return result


@app.post("/apportion/cost")
async def apportion_by_cost(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, asset_cost}]
):
    """Apportion costs by cost/value of assets."""
    result = ApportionmentResult(cost_type=cost_type, total_cost=total_cost, basis="cost")

    total_asset_cost = sum(c.get("asset_cost", 0) for c in cost_centre_data)

    for centre in cost_centre_data:
        asset_cost = centre.get("asset_cost", 0)
        if total_asset_cost > 0:
            proportion = asset_cost / total_asset_cost
            apportioned_cost = total_cost * proportion
            result.cost_centres.append(
                {
                    "cost_centre_id": centre["cost_centre_id"],
                    "cost_centre_name": centre["cost_centre_name"],
                    "asset_cost": asset_cost,
                    "proportion": proportion,
                    "apportioned_cost": apportioned_cost,
                }
            )
            result.total_apportioned += apportioned_cost

    result.unapportioned = total_cost - result.total_apportioned
    apportionment_results.append(result)
    return result


@app.post("/apportion/net-book-value")
async def apportion_by_net_book_value(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, nbv}]
):
    """Apportion costs by net book value of machinery."""
    result = ApportionmentResult(cost_type=cost_type, total_cost=total_cost, basis="net_book_value")

    total_nbv = sum(c.get("net_book_value", 0) for c in cost_centre_data)

    for centre in cost_centre_data:
        nbv = centre.get("net_book_value", 0)
        if total_nbv > 0:
            proportion = nbv / total_nbv
            apportioned_cost = total_cost * proportion
            result.cost_centres.append(
                {
                    "cost_centre_id": centre["cost_centre_id"],
                    "cost_centre_name": centre["cost_centre_name"],
                    "net_book_value": nbv,
                    "proportion": proportion,
                    "apportioned_cost": apportioned_cost,
                }
            )
            result.total_apportioned += apportioned_cost

    result.unapportioned = total_cost - result.total_apportioned
    apportionment_results.append(result)
    return result


@app.post("/apportion/replacement-value")
async def apportion_by_replacement_value(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, replacement_value}]
):
    """Apportion costs by replacement value of assets."""
    result = ApportionmentResult(cost_type=cost_type, total_cost=total_cost, basis="replacement_value")

    total_replacement = sum(c.get("replacement_value", 0) for c in cost_centre_data)

    for centre in cost_centre_data:
        replacement_value = centre.get("replacement_value", 0)
        if total_replacement > 0:
            proportion = replacement_value / total_replacement
            apportioned_cost = total_cost * proportion
            result.cost_centres.append(
                {
                    "cost_centre_id": centre["cost_centre_id"],
                    "cost_centre_name": centre["cost_centre_name"],
                    "replacement_value": replacement_value,
                    "proportion": proportion,
                    "apportioned_cost": apportioned_cost,
                }
            )
            result.total_apportioned += apportioned_cost

    result.unapportioned = total_cost - result.total_apportioned
    apportionment_results.append(result)
    return result


@app.post("/apportion/personnel")
async def apportion_by_personnel(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, number_of_personnel}]
):
    """Apportion costs by number of personnel."""
    result = ApportionmentResult(cost_type=cost_type, total_cost=total_cost, basis="number_of_personnel")

    total_personnel = sum(c.get("number_of_personnel", 0) for c in cost_centre_data)

    for centre in cost_centre_data:
        personnel = centre.get("number_of_personnel", 0)
        if total_personnel > 0:
            proportion = personnel / total_personnel
            apportioned_cost = total_cost * proportion
            result.cost_centres.append(
                {
                    "cost_centre_id": centre["cost_centre_id"],
                    "cost_centre_name": centre["cost_centre_name"],
                    "number_of_personnel": personnel,
                    "proportion": proportion,
                    "apportioned_cost": apportioned_cost,
                }
            )
            result.total_apportioned += apportioned_cost

    result.unapportioned = total_cost - result.total_apportioned
    apportionment_results.append(result)
    return result


@app.post("/apportion/requisitions")
async def apportion_by_requisitions(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, number_of_requisitions}]
):
    """Apportion costs by number of requisitions."""
    result = ApportionmentResult(cost_type=cost_type, total_cost=total_cost, basis="number_of_requisitions")

    total_requisitions = sum(c.get("number_of_requisitions", 0) for c in cost_centre_data)

    for centre in cost_centre_data:
        requisitions = centre.get("number_of_requisitions", 0)
        if total_requisitions > 0:
            proportion = requisitions / total_requisitions
            apportioned_cost = total_cost * proportion
            result.cost_centres.append(
                {
                    "cost_centre_id": centre["cost_centre_id"],
                    "cost_centre_name": centre["cost_centre_name"],
                    "number_of_requisitions": requisitions,
                    "proportion": proportion,
                    "apportioned_cost": apportioned_cost,
                }
            )
            result.total_apportioned += apportioned_cost

    result.unapportioned = total_cost - result.total_apportioned
    apportionment_results.append(result)
    return result


@app.get("/results")
async def list_apportionment_results(cost_type: Optional[str] = None):
    """List apportionment results."""
    result = apportionment_results
    if cost_type:
        result = [r for r in result if r.cost_type == cost_type]
    return {"results": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
