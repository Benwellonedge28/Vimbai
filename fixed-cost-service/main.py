"""
FinAcc Fixed Cost Service
Manages fixed cost classification and analysis.
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

SERVICE_NAME = "fixed-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8062"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Fixed Cost Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class CostBehavior(str):
    FIXED = "fixed"
    VARIABLE = "variable"
    SEMI_VARIABLE = "semi_variable"
    STEP_FIXED = "step_fixed"


class FixedCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_name: str
    cost_code: str
    amount: float
    cost_behavior: str = "fixed"
    relevant_range_min: float = 0
    relevant_range_max: float = 0
    period_type: str = "monthly"  # daily, weekly, monthly, quarterly, annual
    committed_cost: bool = False  # Committed vs Discretionary
    controllable: bool = True
    department_id: Optional[str] = None
    product_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class FixedCostAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    analysis_period_start: datetime
    analysis_period_end: datetime
    total_fixed_costs: float = 0
    committed_costs: float = 0
    discretionary_costs: float = 0
    controllable_costs: float = 0
    non_controllable_costs: float = 0
    fixed_cost_details: List[Dict[str, Any]] = []
    created_at: datetime = Field(default_factory=datetime.utcnow)


fixed_costs: List[FixedCost] = []
fixed_cost_analyses: List[FixedCostAnalysis] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Fixed cost management"}


@app.post("/costs/add")
async def add_fixed_cost(
    cost_name: str, cost_code: str, amount: float, cost_behavior: str = "fixed",
    period_type: str = "monthly", committed_cost: bool = False, controllable: bool = True,
    department_id: Optional[str] = None, product_id: Optional[str] = None
):
    """Add a fixed cost item."""
    cost = FixedCost(
        cost_name=cost_name, cost_code=cost_code, amount=amount,
        cost_behavior=cost_behavior, period_type=period_type,
        committed_cost=committed_cost, controllable=controllable,
        department_id=department_id, product_id=product_id
    )
    fixed_costs.append(cost)
    return cost


@app.post("/costs/{cost_id}/update")
async def update_fixed_cost(
    cost_id: str, amount: Optional[float] = None,
    committed_cost: Optional[bool] = None, controllable: Optional[bool] = None
):
    """Update fixed cost details."""
    cost = next((c for c in fixed_costs if c.id == cost_id), None)
    if not cost:
        return {"error": "Cost not found"}

    if amount is not None:
        cost.amount = amount
    if committed_cost is not None:
        cost.committed_cost = committed_cost
    if controllable is not None:
        cost.controllable = controllable

    return cost


@app.post("/analyze")
async def analyze_fixed_costs(
    analysis_period_start: datetime, analysis_period_end: datetime,
    department_id: Optional[str] = None, product_id: Optional[str] = None
):
    """Analyze fixed costs for a period."""
    costs = fixed_costs
    if department_id:
        costs = [c for c in costs if c.department_id == department_id]
    if product_id:
        costs = [c for c in costs if c.product_id == product_id]

    analysis = FixedCostAnalysis(
        analysis_period_start=analysis_period_start,
        analysis_period_end=analysis_period_end
    )

    for cost in costs:
        cost_detail = {
            "cost_id": cost.id,
            "cost_name": cost.cost_name,
            "cost_code": cost.cost_code,
            "amount": cost.amount,
            "committed": cost.committed_cost,
            "controllable": cost.controllable
        }
        analysis.fixed_cost_details.append(cost_detail)
        analysis.total_fixed_costs += cost.amount

        if cost.committed_cost:
            analysis.committed_costs += cost.amount
        else:
            analysis.discretionary_costs += cost.amount

        if cost.controllable:
            analysis.controllable_costs += cost.amount
        else:
            analysis.non_controllable_costs += cost.amount

    fixed_cost_analyses.append(analysis)
    return analysis


@app.get("/costs")
async def list_fixed_costs(
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
    committed: Optional[bool] = None
):
    """List fixed costs."""
    result = fixed_costs
    if department_id:
        result = [c for c in result if c.department_id == department_id]
    if product_id:
        result = [c for c in result if c.product_id == product_id]
    if committed is not None:
        result = [c for c in result if c.committed_cost == committed]
    return {"fixed_costs": result}


@app.get("/analyses")
async def list_analyses(
    period_start: Optional[datetime] = None,
    period_end: Optional[datetime] = None
):
    """List fixed cost analyses."""
    result = fixed_cost_analyses
    if period_start:
        result = [a for a in result if a.analysis_period_start >= period_start]
    if period_end:
        result = [a for a in result if a.analysis_period_end <= period_end]
    return {"analyses": result}


@app.get("/summary")
async def get_fixed_cost_summary(
    department_id: Optional[str] = None,
    product_id: Optional[str] = None
):
    """Get fixed cost summary."""
    costs = fixed_costs
    if department_id:
        costs = [c for c in costs if c.department_id == department_id]
    if product_id:
        costs = [c for c in costs if c.product_id == product_id]

    return {
        "total_fixed_costs": sum(c.amount for c in costs),
        "committed_costs": sum(c.amount for c in costs if c.committed_cost),
        "discretionary_costs": sum(c.amount for c in costs if not c.committed_cost),
        "controllable_costs": sum(c.amount for c in costs if c.controllable),
        "non_controllable_costs": sum(c.amount for c in costs if not c.controllable),
        "total_cost_items": len(costs)
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)