"""
Vimbai Cost Centre Service
Manages cost centre classification and tracking.
"""

import os
import uuid
from enum import Enum
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "cost-centre-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8069"))
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

app = FastAPI(title="Vimbai Cost Centre Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class CostCentreType(str, Enum):
    PRODUCTION = "production"
    SERVICE = "service"
    ADMINISTRATIVE = "administrative"


class CostCentre(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    centre_code: str
    centre_name: str
    centre_type: str  # production, service, administrative
    department_id: Optional[str] = None
    floor_area: float = 0  # Square meters
    number_of_personnel: int = 0
    number_of_requisitions: int = 0
    machine_value: float = 0  # Value of machinery
    is_service_centre: bool = False
    parent_centre_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CostCentreCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_centre_id: str
    cost_code: str
    cost_description: str
    amount: float
    cost_type: str  # direct, indirect, overhead
    period: str
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CostCentreSummary(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_centre_id: str
    period: str
    total_direct_costs: float = 0
    total_indirect_costs: float = 0
    total_overhead: float = 0
    allocated_service_costs: float = 0
    total_cost: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


cost_centres: List[CostCentre] = []
cost_centre_costs: List[CostCentreCost] = []
cost_centre_summaries: List[CostCentreSummary] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Cost centre management"}


@app.post("/cost-centres/create")
async def create_cost_centre(
    centre_code: str,
    centre_name: str,
    centre_type: str,
    department_id: Optional[str] = None,
    floor_area: float = 0,
    number_of_personnel: int = 0,
    machine_value: float = 0,
    is_service_centre: bool = False,
    parent_centre_id: Optional[str] = None,
):
    """Create a cost centre."""
    centre = CostCentre(
        centre_code=centre_code,
        centre_name=centre_name,
        centre_type=centre_type,
        department_id=department_id,
        floor_area=floor_area,
        number_of_personnel=number_of_personnel,
        machine_value=machine_value,
        is_service_centre=is_service_centre,
        parent_centre_id=parent_centre_id,
    )
    cost_centres.append(centre)
    return centre


@app.post("/cost-centres/{centre_id}/update")
async def update_cost_centre(
    centre_id: str,
    floor_area: Optional[float] = None,
    number_of_personnel: Optional[int] = None,
    number_of_requisitions: Optional[int] = None,
):
    """Update cost centre details."""
    centre = next((c for c in cost_centres if c.id == centre_id), None)
    if not centre:
        return {"error": "Cost centre not found"}

    if floor_area is not None:
        centre.floor_area = floor_area
    if number_of_personnel is not None:
        centre.number_of_personnel = number_of_personnel
    if number_of_requisitions is not None:
        centre.number_of_requisitions = number_of_requisitions

    return centre


@app.post("/costs/add")
async def add_cost_centre_cost(
    cost_centre_id: str, cost_code: str, cost_description: str, amount: float, cost_type: str, period: str
):
    """Add cost to a cost centre."""
    cost = CostCentreCost(
        cost_centre_id=cost_centre_id,
        cost_code=cost_code,
        cost_description=cost_description,
        amount=amount,
        cost_type=cost_type,
        period=period,
    )
    cost_centre_costs.append(cost)
    return cost


@app.post("/summary/generate")
async def generate_cost_centre_summary(cost_centre_id: str, period: str, allocated_service_costs: float = 0):
    """Generate cost centre summary."""
    costs = [c for c in cost_centre_costs if c.cost_centre_id == cost_centre_id and c.period == period]

    summary = CostCentreSummary(
        cost_centre_id=cost_centre_id, period=period, allocated_service_costs=allocated_service_costs
    )

    for cost in costs:
        if cost.cost_type == "direct":
            summary.total_direct_costs += cost.amount
        elif cost.cost_type == "indirect":
            summary.total_indirect_costs += cost.amount
        else:
            summary.total_overhead += cost.amount

    summary.total_cost = (
        summary.total_direct_costs + summary.total_indirect_costs + summary.total_overhead + allocated_service_costs
    )

    cost_centre_summaries.append(summary)
    return summary


@app.get("/cost-centres")
async def list_cost_centres(
    centre_type: Optional[str] = None, department_id: Optional[str] = None, is_service: Optional[bool] = None
):
    """List cost centres."""
    result = cost_centres
    if centre_type:
        result = [c for c in result if c.centre_type == centre_type]
    if department_id:
        result = [c for c in result if c.department_id == department_id]
    if is_service is not None:
        result = [c for c in result if c.is_service_centre == is_service]
    return {"cost_centres": result}


@app.get("/cost-centres/{centre_id}")
async def get_cost_centre(centre_id: str):
    """Get cost centre details."""
    centre = next((c for c in cost_centres if c.id == centre_id), None)
    if not centre:
        return {"error": "Cost centre not found"}

    centre_costs = [c for c in cost_centre_costs if c.cost_centre_id == centre_id]
    return {"cost_centre": centre, "costs": centre_costs, "total_costs": sum(c.amount for c in centre_costs)}


@app.get("/summaries")
async def list_summaries(cost_centre_id: Optional[str] = None, period: Optional[str] = None):
    """List cost centre summaries."""
    result = cost_centre_summaries
    if cost_centre_id:
        result = [s for s in result if s.cost_centre_id == cost_centre_id]
    if period:
        result = [s for s in result if s.period == period]
    return {"summaries": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
