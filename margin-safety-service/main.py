"""
Vimbai Margin of Safety Service
Calculates margin of safety for products and businesses.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "margin_safety_service" not in _sys.modules or not hasattr(_sys.modules.get("margin_safety_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("margin_safety_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["margin_safety_service"] = _pkg
    _sys.modules["margin_safety_service"].__path__ = [_HERE]

import os
from typing import Any, Dict, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from margin_safety_service import calc, crud
from margin_safety_service.dependencies import book_id_var, get_db_session, get_user_id
from margin_safety_service.models import MarginOfSafety
from neo4j import AsyncSession

SERVICE_NAME = "margin-safety-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8082"))
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

app = FastAPI(title="Vimbai Margin of Safety Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Margin of safety calculation"}


@app.post("/calculate")
async def calculate_margin_of_safety(
    entity_id: str,
    entity_name: str,
    period: str,
    current_output: float,
    current_revenue: float,
    break_even_output: float,
    break_even_revenue: float,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Calculate margin of safety."""
    result = calc.margin_of_safety(current_output, current_revenue, break_even_output, break_even_revenue)

    mos = MarginOfSafety(
        entity_id=entity_id,
        entity_name=entity_name,
        period=period,
        current_output=result["current_output"],
        current_revenue=result["current_revenue"],
        break_even_output=result["break_even_output"],
        break_even_revenue=result["break_even_revenue"],
        margin_of_safety_units=result["margin_of_safety_units"],
        margin_of_safety_revenue=result["margin_of_safety_revenue"],
        margin_of_safety_percentage=result["margin_of_safety_percentage"],
        margin_of_safety_ratio=result["margin_of_safety_ratio"],
        risk_level=result["risk_level"],
    )
    return await crud.create(session, user_id, mos)


@app.post("/calculate-from-data")
async def calculate_from_basic_data(
    entity_name: str,
    selling_price: float,
    fixed_costs: float,
    variable_cost_per_unit: float,
    expected_sales_units: float,
):
    """Calculate margin of safety from basic data."""
    result = calc.margin_from_data(selling_price, fixed_costs, variable_cost_per_unit, expected_sales_units)
    return {"entity_name": entity_name, **result}


@app.post("/target-mos")
async def calculate_sales_for_target_mos(entity_name: str, break_even_revenue: float, target_mos_percentage: float):
    """Calculate sales revenue needed for target margin of safety."""
    result = calc.target_mos(break_even_revenue, target_mos_percentage)
    return {"entity_name": entity_name, **result}


@app.get("/margins")
async def list_margins(
    entity_id: Optional[str] = None,
    risk_level: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List margin of safety calculations."""
    result = await crud.list_all(session, user_id, MarginOfSafety)
    if entity_id:
        result = [m for m in result if m.entity_id == entity_id]
    if risk_level:
        result = [m for m in result if m.risk_level == risk_level]
    return {"margins": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
