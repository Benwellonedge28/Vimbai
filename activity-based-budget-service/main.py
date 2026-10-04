"""Vimbai Activity-Based Budget Service. Port: 8177.

Budgets based on activity drivers and cost pools. The two module-level
lists (activities, budgets) were process-global and shared across ALL
callers; they now persist to Neo4j as caller-owned, Book-scoped records
(X-User-Id / X-Book-ID). Activity lookups during budget creation only
see the caller's own activities.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "activity_based_budget_service" not in _sys.modules or not hasattr(
    _sys.modules.get("activity_based_budget_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("activity_based_budget_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["activity_based_budget_service"] = _pkg
    _sys.modules["activity_based_budget_service"].__path__ = [_HERE]

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from activity_based_budget_service import crud
from activity_based_budget_service.dependencies import book_id_var, get_db_session, get_user_id
from activity_based_budget_service.exceptions import ActivityBasedBudgetError
from activity_based_budget_service.models import Activity, ActivityBudget, BudgetLineItem
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(ActivityBasedBudgetError)
async def _abb_error(request: Request, exc: ActivityBasedBudgetError):
    return JSONResponse(
        status_code=getattr(exc, "status_code", 400), content={"detail": str(exc), "error": exc.__class__.__name__}
    )


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/activities", response_model=Activity)
async def create_activity(
    name: str,
    description: str = "",
    cost_pool: str = "",
    driver: str = "",
    driver_rate: float = 0.0,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Define an activity with its cost driver (persisted, caller-owned)."""
    activity = Activity(name=name, description=description, cost_pool=cost_pool, driver=driver, driver_rate=driver_rate)
    await crud.create(db_session, caller_id, activity)
    logger.info("Activity defined", activity_id=activity.id, name=name)
    return activity


@app.get("/activities", response_model=List[Activity])
async def list_activities(caller_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(get_db_session)):
    """List the caller's activities."""
    return await crud.list_all(db_session, caller_id, Activity)


@app.post("/budgets", response_model=ActivityBudget)
async def create_budget(
    name: str,
    fiscal_year: str,
    period: str,
    line_items: List[Dict[str, Any]] = [],
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create an activity-based budget over the caller's own activities."""
    items = []
    for li in line_items:
        activity = await crud.find(db_session, caller_id, Activity, li.get("activity_id"))
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
    await crud.create(db_session, caller_id, budget)
    logger.info("Activity-based budget created", budget_id=budget.id, total=total)
    return budget


@app.get("/budgets", response_model=List[ActivityBudget])
async def list_budgets(
    status: Optional[str] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's activity-based budgets."""
    result = await crud.list_all(db_session, caller_id, ActivityBudget)
    if status:
        result = [b for b in result if b.status == status]
    return result


@app.get("/budgets/{budget_id}", response_model=ActivityBudget)
async def get_budget(
    budget_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get one of the caller's budgets."""
    budget = await crud.find(db_session, caller_id, ActivityBudget, budget_id)
    if not budget:
        raise HTTPException(status_code=404, detail="Budget not found")
    return budget


@app.put("/budgets/{budget_id}/approve")
async def approve_budget(
    budget_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Approve one of the caller's budgets (upsert with new status)."""
    budget = await crud.find(db_session, caller_id, ActivityBudget, budget_id)
    if not budget:
        raise HTTPException(status_code=404, detail="Budget not found")
    budget.status = "approved"
    await crud.delete_where(db_session, caller_id, ActivityBudget, {"id": budget_id})
    await crud.create(db_session, caller_id, budget)
    return {"budget_id": budget_id, "status": "approved"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
