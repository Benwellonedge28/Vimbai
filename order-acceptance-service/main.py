"""
Vimbai Order Acceptance Below Selling Price Service
Analyzes whether to accept orders below normal selling price.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "order_acceptance_service" not in _sys.modules or not hasattr(
    _sys.modules.get("order_acceptance_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("order_acceptance_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["order_acceptance_service"] = _pkg
    _sys.modules["order_acceptance_service"].__path__ = [_HERE]

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "order_acceptance_service" not in _sys.modules or not hasattr(
    _sys.modules.get("order_acceptance_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("order_acceptance_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["order_acceptance_service"] = _pkg
    _sys.modules["order_acceptance_service"].__path__ = [_HERE]

import os
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from order_acceptance_service import calc, crud
from order_acceptance_service.dependencies import book_id_var, get_db_session, get_user_id
from order_acceptance_service.models import OrderAcceptanceDecision
from pydantic import BaseModel, Field

SERVICE_NAME = "order-acceptance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8077"))
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

app = FastAPI(title="Vimbai Order Acceptance Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Order acceptance analysis"}


@app.post("/analyze")
async def analyze_order_acceptance(
    order_id: str,
    product_id: str,
    product_name: str,
    customer_name: str,
    normal_selling_price: float,
    offered_price: float,
    variable_cost_per_unit: float,
    units_requested: float,
    full_cost_per_unit: Optional[float] = None,
    incremental_fixed_costs: float = 0,
    displaced_sales_units: float = 0,
    displaced_contribution_per_unit: float = 0,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Analyze order acceptance decision."""
    figures = calc.analyze(
        order_id,
        product_id,
        product_name,
        customer_name,
        normal_selling_price,
        offered_price,
        variable_cost_per_unit,
        units_requested,
        full_cost_per_unit,
        incremental_fixed_costs,
        displaced_sales_units,
        displaced_contribution_per_unit,
    )
    full_cost = figures.pop("full_cost_per_unit")
    decision = OrderAcceptanceDecision(**figures)
    if full_cost is not None:
        decision.full_cost_per_unit = full_cost

    return await crud.create(session, user_id, decision)


@app.post("/one-time-order")
async def analyze_one_time_order(
    product_name: str,
    offered_price: float,
    variable_cost_per_unit: float,
    units: float,
    has_spare_capacity: bool = True,
):
    """Quick analysis for one-time special order."""
    return calc.one_time_order(product_name, offered_price, variable_cost_per_unit, units, has_spare_capacity)


@app.get("/decisions")
async def list_decisions(
    order_id: Optional[str] = None,
    product_id: Optional[str] = None,
    recommendation: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List order acceptance decisions."""
    result = await crud.list_all(session, user_id, OrderAcceptanceDecision)
    if order_id:
        result = [d for d in result if d.order_id == order_id]
    if product_id:
        result = [d for d in result if d.product_id == product_id]
    if recommendation:
        result = [d for d in result if d.recommendation == recommendation]
    return {"decisions": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
