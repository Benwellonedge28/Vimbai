"""Vimbai Fixed Cost Service - classify and analyse fixed costs. Port: 8062

Costs and the analyses run against them live in Neo4j, scoped to the caller and
the Book. The file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "fixed_cost_service" not in _sys.modules or not hasattr(_sys.modules.get("fixed_cost_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("fixed_cost_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["fixed_cost_service"] = _pkg
    _sys.modules["fixed_cost_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import Optional

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fixed_cost_service import crud
from fixed_cost_service.dependencies import book_id_var, get_db_session, get_user_id
from neo4j import AsyncSession

SERVICE_NAME = "fixed-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8062"))

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

app = FastAPI(title="Vimbai Fixed Cost Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Fixed cost management"}


@app.post("/costs/add")
async def add_fixed_cost(
    cost_name: str,
    cost_code: str,
    amount: float,
    cost_behavior: str = "fixed",
    period_type: str = "monthly",
    committed_cost: bool = False,
    controllable: bool = True,
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a fixed cost item."""
    cost = await crud.add_cost(
        db_session,
        user_id,
        cost_name=cost_name,
        cost_code=cost_code,
        amount=amount,
        cost_behavior=cost_behavior,
        period_type=period_type,
        committed_cost=committed_cost,
        controllable=controllable,
        department_id=department_id,
        product_id=product_id,
    )
    logger.info("fixed_cost_added", cost_code=cost_code, amount=amount)
    return cost


@app.post("/costs/{cost_id}/update")
async def update_fixed_cost(
    cost_id: str,
    amount: Optional[float] = None,
    committed_cost: Optional[bool] = None,
    controllable: Optional[bool] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update fixed cost details."""
    try:
        return await crud.update_cost(
            db_session, user_id, cost_id, amount=amount, committed_cost=committed_cost, controllable=controllable
        )
    except crud.NotFoundError:
        # the original answers 200 with this body rather than a 404
        return {"error": "Cost not found"}


@app.post("/analyze")
async def analyze_fixed_costs(
    analysis_period_start: datetime,
    analysis_period_end: datetime,
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Analyze fixed costs for a period."""
    analysis = await crud.analyse_costs(
        db_session,
        user_id,
        analysis_period_start,
        analysis_period_end,
        department_id or "",
        product_id or "",
    )
    logger.info("fixed_cost_analysed", total=analysis.total_fixed_costs, items=len(analysis.fixed_cost_details))
    return analysis


@app.get("/costs")
async def list_fixed_costs(
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
    committed: Optional[bool] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List fixed costs."""
    costs = await crud.list_costs(db_session, user_id, department_id or "", product_id or "", committed)
    return {"fixed_costs": costs}


@app.get("/analyses")
async def list_analyses(
    period_start: Optional[datetime] = None,
    period_end: Optional[datetime] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List fixed cost analyses."""
    return {"analyses": await crud.list_analyses(db_session, user_id, period_start, period_end)}


@app.get("/summary")
async def get_fixed_cost_summary(
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get fixed cost summary."""
    costs = await crud.list_costs(db_session, user_id, department_id or "", product_id or "")
    return crud.summarise(costs)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
