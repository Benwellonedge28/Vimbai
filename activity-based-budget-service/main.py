"""
Vimbai Activity-Based Budget Service
Creates budgets based on activity drivers and cost pools.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "activity-based-budget-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8177"))

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

app = FastAPI(title="Vimbai Activity-Based Budget Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class Activity(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    cost_pool: str
    driver: str  # e.g. machine_hours, labor_hours, transactions, setups
    driver_rate: float = 0.0


class BudgetLineItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    activity_id: str
    period: str  # YYYY-MM
    expected_driver_volume: float
    budgeted_cost: float = 0.0
    notes: str = ""


class ActivityBudget(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    fiscal_year: str
    period: str  # YYYY-MM or full year
    line_items: List[BudgetLineItem] = []
    total_budget: float = 0.0
    status: str = "draft"  # draft, approved, actual
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


activities: List[Activity] = []
budgets: List[ActivityBudget] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/activities", response_model=Activity)
async def create_activity(
    name: str, description: str = "", cost_pool: str = "", driver: str = "", driver_rate: float = 0.0
):
    """Define an activity with its cost driver."""
    activity = Activity(name=name, description=description, cost_pool=cost_pool, driver=driver, driver_rate=driver_rate)
    activities.append(activity)
    logger.info("Activity defined", activity_id=activity.id, name=name)
    return activity


@app.get("/activities", response_model=List[Activity])
async def list_activities():
    """List all activities."""
    return activities


@app.post("/budgets", response_model=ActivityBudget)
async def create_budget(name: str, fiscal_year: str, period: str, line_items: List[Dict[str, Any]] = []):
    """Create an activity-based budget."""
    items = []
    for li in line_items:
        activity = next((a for a in activities if a.id == li.get("activity_id")), None)
        if not activity:
            raise HTTPException(status_code=404, detail=f"Activity {li.get('activity_id')} not found")

        volume = li.get("expected_driver_volume", 0)
        budgeted_cost = volume * activity.driver_rate
        item = BudgetLineItem(
            activity_id=li["activity_id"],
            period=period,
            expected_driver_volume=volume,
            budgeted_cost=budgeted_cost,
            notes=li.get("notes", ""),
        )
        items.append(item)

    total = sum(i.budgeted_cost for i in items)
    budget = ActivityBudget(
        name=name,
        fiscal_year=fiscal_year,
        period=period,
        line_items=items,
        total_budget=total,
    )
    budgets.append(budget)
    logger.info("Activity-based budget created", budget_id=budget.id, total=total)
    return budget


@app.get("/budgets", response_model=List[ActivityBudget])
async def list_budgets(status: Optional[str] = None):
    """List activity-based budgets."""
    if status:
        return [b for b in budgets if b.status == status]
    return budgets


@app.get("/budgets/{budget_id}", response_model=ActivityBudget)
async def get_budget(budget_id: str):
    """Get a specific budget."""
    budget = next((b for b in budgets if b.id == budget_id), None)
    if not budget:
        raise HTTPException(status_code=404, detail="Budget not found")
    return budget


@app.put("/budgets/{budget_id}/approve")
async def approve_budget(budget_id: str):
    """Approve a budget."""
    budget = next((b for b in budgets if b.id == budget_id), None)
    if not budget:
        raise HTTPException(status_code=404, detail="Budget not found")
    budget.status = "approved"
    return {"budget_id": budget_id, "status": "approved"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
