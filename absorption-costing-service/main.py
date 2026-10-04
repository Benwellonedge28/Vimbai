"""Vimbai Absorption Costing Service. Port: 8064.

Total costing / absorption costing methods. The two module-level lists
(product_costs, overhead_absorptions) were process-global and shared
across ALL callers; they now persist to Neo4j as caller-owned,
Book-scoped records (X-User-Id / X-Book-ID). Cost-plus pricing stays
a pure calculation.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "absorption_costing_service" not in _sys.modules or not hasattr(
    _sys.modules.get("absorption_costing_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("absorption_costing_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["absorption_costing_service"] = _pkg
    _sys.modules["absorption_costing_service"].__path__ = [_HERE]

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from absorption_costing_service import crud
from absorption_costing_service.dependencies import book_id_var, get_db_session, get_user_id
from absorption_costing_service.exceptions import AbsorptionCostingError
from absorption_costing_service.models import CostComponent, OverheadAbsorption, ProductCost
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession
from pydantic import BaseModel

SERVICE_NAME = "absorption-costing-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8064"))
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

app = FastAPI(title="Vimbai Absorption Costing Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(AbsorptionCostingError)
async def _absorption_costing_error(request: Request, exc: AbsorptionCostingError):
    return JSONResponse(
        status_code=getattr(exc, "status_code", 400), content={"detail": str(exc), "error": exc.__class__.__name__}
    )


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Absorption costing management"}


@app.post("/product-costs/calculate")
async def calculate_product_cost(
    product_id: str,
    product_name: str,
    period: str,
    direct_materials: float,
    direct_labor: float,
    direct_expenses: float,
    manufacturing_overhead: float,
    units_produced: int,
    opening_stock: int = 0,
    closing_stock: int = 0,
    cost_components: Optional[List[Dict[str, Any]]] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate full product cost using absorption costing (persisted, caller-owned)."""
    product_cost = ProductCost(
        product_id=product_id,
        product_name=product_name,
        period=period,
        direct_materials=direct_materials,
        direct_labor=direct_labor,
        direct_expenses=direct_expenses,
        manufacturing_overhead=manufacturing_overhead,
        units_produced=units_produced,
        opening_stock=opening_stock,
        closing_stock=closing_stock,
    )

    # Calculate prime cost
    product_cost.prime_cost = direct_materials + direct_labor + direct_expenses

    # Calculate total production cost
    product_cost.total_production_cost = product_cost.prime_cost + manufacturing_overhead

    # Calculate cost per unit
    if units_produced > 0:
        product_cost.cost_per_unit = product_cost.total_production_cost / units_produced

    # Add cost components if provided
    if cost_components:
        for comp in cost_components:
            product_cost.cost_components.append(CostComponent(**comp))

    # Create journal entry
    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Product cost calculation - {product_name} ({period})",
        "entries": [
            {
                "account_code": "1500",
                "description": "Work in Progress",
                "debit": product_cost.total_production_cost,
                "credit": 0,
            },
            {"account_code": "1100", "description": "Raw Materials", "debit": 0, "credit": direct_materials},
            {"account_code": "2100", "description": "Direct Labor", "debit": 0, "credit": direct_labor},
            {
                "account_code": "2200",
                "description": "Manufacturing Overhead",
                "debit": 0,
                "credit": manufacturing_overhead,
            },
        ],
        "reference": f"ABS-COST-{product_cost.id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    product_cost.journal_entry_id = result.get("id")
    await crud.create(db_session, caller_id, product_cost)

    return product_cost


@app.post("/overhead/absorption")
async def calculate_overhead_absorption(
    product_id: str,
    period: str,
    overhead_cost: float,
    absorption_base: str,
    absorption_base_units: float,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate overhead absorption rate and absorbed overhead (persisted, caller-owned)."""
    absorption = OverheadAbsorption(
        product_id=product_id,
        period=period,
        overhead_cost=overhead_cost,
        absorption_base=absorption_base,
        absorption_base_units=absorption_base_units,
    )

    # Calculate overhead absorption rate
    if absorption_base_units > 0:
        absorption.overhead_absorption_rate = overhead_cost / absorption_base_units
        absorption.absorbed_overhead = absorption.overhead_absorption_rate * absorption_base_units

    await crud.create(db_session, caller_id, absorption)
    return absorption


@app.post("/cost-plus")
async def calculate_cost_plus_pricing(product_cost: float, markup_percentage: float):
    """Calculate selling price using cost-plus pricing (pure calculation)."""
    markup_amount = product_cost * (markup_percentage / 100)
    selling_price = product_cost + markup_amount

    return {
        "product_cost": product_cost,
        "markup_percentage": markup_percentage,
        "markup_amount": markup_amount,
        "selling_price": selling_price,
    }


@app.get("/product-costs")
async def list_product_costs(
    product_id: Optional[str] = None,
    period: Optional[str] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's product costs."""
    result = await crud.list_all(db_session, caller_id, ProductCost)
    if product_id:
        result = [p for p in result if p.product_id == product_id]
    if period:
        result = [p for p in result if p.period == period]
    return {"product_costs": result}


@app.get("/product-costs/{product_id}/latest")
async def get_latest_product_cost(
    product_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's latest product cost for a product."""
    product_cost = next(
        (p for p in reversed(await crud.list_all(db_session, caller_id, ProductCost)) if p.product_id == product_id),
        None,
    )
    if not product_cost:
        return {"error": "Product cost not found"}
    return product_cost


@app.get("/stock-valuation")
async def calculate_stock_valuation(
    product_id: str,
    valuation_method: str = "fifo",
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate stock valuation using the caller's absorption costing records."""
    product_cost = next(
        (p for p in reversed(await crud.list_all(db_session, caller_id, ProductCost)) if p.product_id == product_id),
        None,
    )
    if not product_cost:
        return {"error": "Product cost not found"}

    closing_stock_value = product_cost.cost_per_unit * product_cost.closing_stock

    return {
        "product_id": product_id,
        "valuation_method": valuation_method,
        "cost_per_unit": product_cost.cost_per_unit,
        "closing_stock_units": product_cost.closing_stock,
        "closing_stock_value": closing_stock_value,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
