"""
Vimbai Total Production Cost Service
Calculates total cost of production (Prime Cost + Factory Overhead).
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "total_production_cost_service" not in _sys.modules or not hasattr(
    _sys.modules.get("total_production_cost_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("total_production_cost_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["total_production_cost_service"] = _pkg
    _sys.modules["total_production_cost_service"].__path__ = [_HERE]

import os
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from total_production_cost_service import calc, crud
from total_production_cost_service.dependencies import book_id_var, get_db_session, get_user_id
from total_production_cost_service.models import ProductionCostCalculation

SERVICE_NAME = "total-production-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8068"))
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

app = FastAPI(title="Vimbai Total Production Cost Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


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
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "status": "healthy",
        "calculation_engine": calc.engine_in_use(),
    }


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Total production cost calculation"}


@app.post("/calculate", response_model=ProductionCostCalculation)
async def calculate_total_production_cost(
    product_id: str,
    department_id: str,
    period: str,
    direct_materials: float,
    direct_labor: float,
    direct_expenses: float = 0,
    factory_rent: float = 0,
    factory_depreciation: float = 0,
    factory_insurance: float = 0,
    factory_maintenance: float = 0,
    other_overhead: float = 0,
    wip_opening: float = 0,
    wip_closing: float = 0,
    units_produced: int = 0,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Calculate total production cost."""
    figures = calc.calculate(
        product_id,
        department_id,
        period,
        direct_materials,
        direct_labor,
        direct_expenses,
        factory_rent,
        factory_depreciation,
        factory_insurance,
        factory_maintenance,
        other_overhead,
        wip_opening,
        wip_closing,
        units_produced,
    )
    record = ProductionCostCalculation(**figures)
    return await crud.create(session, user_id, record)


@app.get("/calculations")
async def list_calculations(
    product_id: Optional[str] = None,
    department_id: Optional[str] = None,
    period: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List production cost calculations."""
    result = await crud.list_all(session, user_id, ProductionCostCalculation)
    if product_id:
        result = [c for c in result if c.product_id == product_id]
    if department_id:
        result = [c for c in result if c.department_id == department_id]
    if period:
        result = [c for c in result if c.period == period]
    return {"calculations": result}


@app.get("/calculations/{product_id}/latest")
async def get_latest_calculation(
    product_id: str,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Get latest production cost calculation."""
    calcs = await crud.list_all(session, user_id, ProductionCostCalculation)
    matching = [c for c in calcs if c.product_id == product_id]
    if not matching:
        return {"error": "Calculation not found"}
    return max(matching, key=lambda c: c.created_at)


@app.get("/summary")
async def get_production_cost_summary(
    department_id: Optional[str] = None,
    period: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Get summary of production costs."""
    calcs = await crud.list_all(session, user_id, ProductionCostCalculation)
    if department_id:
        calcs = [c for c in calcs if c.department_id == department_id]
    if period:
        calcs = [c for c in calcs if c.period == period]

    total_production_cost = sum(c.total_production_cost for c in calcs)
    total_units = sum(c.units_produced for c in calcs)
    return {
        "total_prime_cost": sum(c.prime_cost for c in calcs),
        "total_overhead": sum(c.total_overhead for c in calcs),
        "total_production_cost": total_production_cost,
        "total_units_produced": total_units,
        "average_cost_per_unit": (total_production_cost / total_units if total_units > 0 else 0),
        "calculations_count": len(calcs),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
