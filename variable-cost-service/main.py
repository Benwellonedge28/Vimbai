"""Vimbai Variable Cost Service - unit driven costs and their analyses. Port: 8063

Costs and analyses live in Neo4j, scoped to the caller and the Book. The file may
be imported bare (bracket mounts, uvicorn main:app), so it bootstraps its own
package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "variable_cost_service" not in _sys.modules or not hasattr(_sys.modules.get("variable_cost_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("variable_cost_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["variable_cost_service"] = _pkg
    _sys.modules["variable_cost_service"].__path__ = [_HERE]

import os
from typing import Optional

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from variable_cost_service import crud
from variable_cost_service.dependencies import book_id_var, get_db_session, get_user_id

SERVICE_NAME = "variable-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8063"))

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
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a variable cost item."""
    cost = await crud.add_cost(
        db_session,
        user_id,
        cost_name=cost_name,
        cost_code=cost_code,
        unit_cost=unit_cost,
        quantity=quantity,
        cost_driver=cost_driver,
        department_id=department_id,
        product_id=product_id,
    )
    logger.info("variable_cost_added", cost_code=cost_code, total=cost.total_cost)
    return cost


@app.post("/costs/{cost_id}/update")
async def update_variable_cost(
    cost_id: str,
    unit_cost: Optional[float] = None,
    quantity: Optional[float] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update variable cost details."""
    try:
        return await crud.update_cost(db_session, user_id, cost_id, unit_cost=unit_cost, quantity=quantity)
    except crud.NotFoundError:
        # the original answers 200 with this body rather than a 404
        return {"error": "Cost not found"}


@app.post("/analyze")
async def analyze_variable_costs(
    analysis_period: str,
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Analyze variable costs for a period."""
    analysis = await crud.analyse_costs(db_session, user_id, analysis_period, department_id or "", product_id or "")
    logger.info("variable_cost_analysed", period=analysis_period, total=analysis.total_variable_costs)
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
async def list_variable_costs(
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List variable costs."""
    costs = await crud.list_costs(db_session, user_id, department_id or "", product_id or "")
    return {"variable_costs": costs}


@app.get("/summary")
async def get_variable_cost_summary(
    department_id: Optional[str] = None,
    product_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get variable cost summary."""
    costs = await crud.list_costs(db_session, user_id, department_id or "", product_id or "")
    return crud.summarise(costs)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
