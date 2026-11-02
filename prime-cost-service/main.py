"""
Vimbai Prime Cost Service
Calculates prime cost (direct materials + direct labor + direct expenses).
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "prime_costing_service" not in _sys.modules or not hasattr(_sys.modules.get("prime_costing_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("prime_costing_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["prime_costing_service"] = _pkg
    _sys.modules["prime_costing_service"].__path__ = [_HERE]

import os
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from prime_costing_service import calc, crud
from prime_costing_service.dependencies import book_id_var, get_db_session, get_user_id
from prime_costing_service.models import DirectCostItem, PrimeCostCalculation

SERVICE_NAME = "prime-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8067"))
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

app = FastAPI(title="Vimbai Prime Cost Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Prime cost calculation"}


@app.post("/direct-costs/add")
async def add_direct_cost_item(
    item_name: str,
    item_type: str,
    amount: float,
    units: float = 1,
    product_id: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Add a direct cost item."""
    item = DirectCostItem(item_name=item_name, item_type=item_type, amount=amount, units=units, product_id=product_id)
    item.cost_per_unit = amount / units if units > 0 else 0
    return await crud.create(session, user_id, item)


@app.post("/calculate")
async def calculate_prime_cost(
    product_id: str,
    period: str,
    direct_materials: float,
    direct_labor: float,
    direct_expenses: float = 0,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Calculate prime cost for a product."""
    calculation = PrimeCostCalculation(
        product_id=product_id,
        period=period,
        direct_materials=direct_materials,
        direct_labor=direct_labor,
        direct_expenses=direct_expenses,
    )

    figures = calc.prime_cost(product_id, period, direct_materials, direct_labor, direct_expenses)
    calculation.prime_cost = figures["prime_cost"]
    calculation.cost_breakdown = figures["cost_breakdown"]

    return await crud.create(session, user_id, calculation)


@app.post("/calculate-with-items")
async def calculate_prime_cost_from_items(
    product_id: str,
    period: str,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Calculate prime cost from direct cost items."""
    items = await crud.list_all(session, user_id, DirectCostItem)
    figures = calc.prime_cost_from_items(product_id, period, items)

    calculation = PrimeCostCalculation(
        product_id=product_id,
        period=period,
        direct_materials=figures["direct_materials"],
        direct_labor=figures["direct_labor"],
        direct_expenses=figures["direct_expenses"],
    )
    calculation.prime_cost = figures["prime_cost"]
    calculation.cost_breakdown = figures["cost_breakdown"]

    return await crud.create(session, user_id, calculation)


@app.get("/calculations")
async def list_calculations(
    product_id: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List prime cost calculations."""
    result = await crud.list_all(session, user_id, PrimeCostCalculation)
    if product_id:
        result = [c for c in result if c.product_id == product_id]
    return {"calculations": result}


@app.get("/direct-costs")
async def list_direct_cost_items(
    product_id: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List direct cost items."""
    result = await crud.list_all(session, user_id, DirectCostItem)
    if product_id:
        result = [i for i in result if i.product_id == product_id]
    return {"items": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
