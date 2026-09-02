"""
Vimbai Variable Cost Service
Manages variable cost classification and analysis.
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

SERVICE_NAME = "variable-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8063"))
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

app = FastAPI(title="Vimbai Variable Cost Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class VariableCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_name: str
    cost_code: str
    unit_cost: float  # Cost per unit
    total_cost: float = 0
    quantity: float = 0
    cost_driver: str  # What drives this cost (units, labor hours, machine hours, etc.)
    cost_driver_rate: float  # Rate per cost driver unit
    department_id: Optional[str] = None
    product_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class VariableCostAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    analysis_period: str
    total_variable_costs: float = 0
    total_units: float = 0
    average_cost_per_unit: float = 0
    cost_driver_analysis: Dict[str, float] = {}
    product_costs: Dict[str, float] = {}  # product_id -> total cost
    created_at: datetime = Field(default_factory=datetime.utcnow)


variable_costs: List[VariableCost] = []
variable_cost_analyses: List[VariableCostAnalysis] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Variable cost management"}


@app.post("/costs/add")
async def add_variable_cost(
    cost_name: str,
    cost_code: str,
    unit_cost: float,
    quantity: float,
    cost_driver: str,
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
):
    """Add a variable cost item."""
    cost = VariableCost(
        cost_name=cost_name,
        cost_code=cost_code,
        unit_cost=unit_cost,
        quantity=quantity,
        cost_driver=cost_driver,
        department_id=department_id,
        product_id=product_id,
    )
    cost.total_cost = unit_cost * quantity
    cost.cost_driver_rate = unit_cost
    variable_costs.append(cost)
    return cost


@app.post("/costs/{cost_id}/update")
async def update_variable_cost(cost_id: str, unit_cost: Optional[float] = None, quantity: Optional[float] = None):
    """Update variable cost details."""
    cost = next((c for c in variable_costs if c.id == cost_id), None)
    if not cost:
        return {"error": "Cost not found"}

    if unit_cost is not None:
        cost.unit_cost = unit_cost
    if quantity is not None:
        cost.quantity = quantity

    cost.total_cost = cost.unit_cost * cost.quantity
    return cost


@app.post("/analyze")
async def analyze_variable_costs(
    analysis_period: str, department_id: Optional[str] = None, product_id: Optional[str] = None
):
    """Analyze variable costs for a period."""
    costs = variable_costs
    if department_id:
        costs = [c for c in costs if c.department_id == department_id]
    if product_id:
        costs = [c for c in costs if c.product_id == product_id]

    analysis = VariableCostAnalysis(analysis_period=analysis_period)

    for cost in costs:
        analysis.total_variable_costs += cost.total_cost
        analysis.total_units += cost.quantity

        if cost.product_id:
            if cost.product_id not in analysis.product_costs:
                analysis.product_costs[cost.product_id] = 0
            analysis.product_costs[cost.product_id] += cost.total_cost

        if cost.cost_driver not in analysis.cost_driver_analysis:
            analysis.cost_driver_analysis[cost.cost_driver] = 0
        analysis.cost_driver_analysis[cost.cost_driver] += cost.total_cost

    if analysis.total_units > 0:
        analysis.average_cost_per_unit = analysis.total_variable_costs / analysis.total_units

    variable_cost_analyses.append(analysis)
    return analysis


@app.post("/calculate-total")
async def calculate_total_variable_cost(unit_cost: float, expected_quantity: float):
    """Calculate total variable cost for expected production."""
    total_cost = unit_cost * expected_quantity
    return {
        "unit_cost": unit_cost,
        "expected_quantity": expected_quantity,
        "total_variable_cost": total_cost,
        "cost_per_driver_unit": unit_cost,
    }


@app.get("/costs")
async def list_variable_costs(department_id: Optional[str] = None, product_id: Optional[str] = None):
    """List variable costs."""
    result = variable_costs
    if department_id:
        result = [c for c in result if c.department_id == department_id]
    if product_id:
        result = [c for c in result if c.product_id == product_id]
    return {"variable_costs": result}


@app.get("/summary")
async def get_variable_cost_summary(department_id: Optional[str] = None, product_id: Optional[str] = None):
    """Get variable cost summary."""
    costs = variable_costs
    if department_id:
        costs = [c for c in costs if c.department_id == department_id]
    if product_id:
        costs = [c for c in costs if c.product_id == product_id]

    total_units = sum(c.quantity for c in costs)
    total_cost = sum(c.total_cost for c in costs)

    return {
        "total_variable_costs": total_cost,
        "total_units": total_units,
        "average_cost_per_unit": total_cost / total_units if total_units > 0 else 0,
        "cost_drivers": list(set(c.cost_driver for c in costs)),
        "total_cost_items": len(costs),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
