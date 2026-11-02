"""
Vimbai Cost Centre Service
Manages cost centre classification and tracking.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "cost_centre_service" not in _sys.modules or not hasattr(_sys.modules.get("cost_centre_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("cost_centre_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["cost_centre_service"] = _pkg
    _sys.modules["cost_centre_service"].__path__ = [_HERE]

import os
from typing import Any, Dict, List, Optional

import httpx
import structlog
from cost_centre_service import calc, crud
from cost_centre_service.dependencies import book_id_var, get_db_session, get_user_id
from cost_centre_service.models import CostCentre, CostCentreCost, CostCentreSummary, CostCentreType
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/health")
async def health_check():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "status": "healthy",
        "calculation_engine": calc.engine_in_use(),
    }


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
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
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
    return await crud.create(session, user_id, centre)


@app.post("/cost-centres/{centre_id}/update")
async def update_cost_centre(
    centre_id: str,
    floor_area: Optional[float] = None,
    number_of_personnel: Optional[int] = None,
    number_of_requisitions: Optional[int] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Update cost centre details."""
    centre = await crud.find(session, user_id, CostCentre, centre_id)
    if not centre:
        return {"error": "Cost centre not found"}

    if floor_area is not None:
        centre.floor_area = floor_area
    if number_of_personnel is not None:
        centre.number_of_personnel = number_of_personnel
    if number_of_requisitions is not None:
        centre.number_of_requisitions = number_of_requisitions

    return await crud.replace(session, user_id, centre)


@app.post("/costs/add")
async def add_cost_centre_cost(
    cost_centre_id: str,
    cost_code: str,
    cost_description: str,
    amount: float,
    cost_type: str,
    period: str,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
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
    return await crud.create(session, user_id, cost)


@app.post("/summary/generate")
async def generate_cost_centre_summary(
    cost_centre_id: str,
    period: str,
    allocated_service_costs: float = 0,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Generate cost centre summary."""
    costs = [
        c.model_dump(mode="json")
        for c in await crud.list_all(session, user_id, CostCentreCost)
        if c.cost_centre_id == cost_centre_id
    ]
    result = calc.summary(costs, cost_centre_id, period, allocated_service_costs)

    summary = CostCentreSummary(
        cost_centre_id=result["cost_centre_id"],
        period=result["period"],
        total_direct_costs=result["total_direct_costs"],
        total_indirect_costs=result["total_indirect_costs"],
        total_overhead=result["total_overhead"],
        allocated_service_costs=result["allocated_service_costs"],
        total_cost=result["total_cost"],
    )
    return await crud.create(session, user_id, summary)


@app.get("/cost-centres")
async def list_cost_centres(
    centre_type: Optional[str] = None,
    department_id: Optional[str] = None,
    is_service: Optional[bool] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List cost centres."""
    result = await crud.list_all(session, user_id, CostCentre)
    if centre_type:
        result = [c for c in result if c.centre_type == centre_type]
    if department_id:
        result = [c for c in result if c.department_id == department_id]
    if is_service is not None:
        result = [c for c in result if c.is_service_centre == is_service]
    return {"cost_centres": result}


@app.get("/cost-centres/{centre_id}")
async def get_cost_centre(
    centre_id: str,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Get cost centre details."""
    centre = await crud.find(session, user_id, CostCentre, centre_id)
    if not centre:
        return {"error": "Cost centre not found"}

    centre_costs = [c for c in await crud.list_all(session, user_id, CostCentreCost) if c.cost_centre_id == centre_id]
    return {"cost_centre": centre, "costs": centre_costs, "total_costs": sum(c.amount for c in centre_costs)}


@app.get("/summaries")
async def list_summaries(
    cost_centre_id: Optional[str] = None,
    period: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List cost centre summaries."""
    result = await crud.list_all(session, user_id, CostCentreSummary)
    if cost_centre_id:
        result = [s for s in result if s.cost_centre_id == cost_centre_id]
    if period:
        result = [s for s in result if s.period == period]
    return {"summaries": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
