"""
Vimbai Limiting Factor Service
Optimizes production when resources are scarce.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "limiting_factor_service" not in _sys.modules or not hasattr(
    _sys.modules.get("limiting_factor_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("limiting_factor_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["limiting_factor_service"] = _pkg
    _sys.modules["limiting_factor_service"].__path__ = [_HERE]

import os
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from limiting_factor_service import calc, crud
from limiting_factor_service.dependencies import book_id_var, get_db_session, get_user_id
from limiting_factor_service.models import (
    LimitingFactorAnalysis,
    LimitingFactorType,
    ProductWithFactor,
)
from neo4j import AsyncSession
from pydantic import BaseModel, Field

SERVICE_NAME = "limiting-factor-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8075"))
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

app = FastAPI(title="Vimbai Limiting Factor Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Limiting factor analysis"}


@app.post("/analyze")
async def analyze_limiting_factor(
    limiting_factor: str,
    factor_type: str,
    total_available: float,
    products: List[Dict[str, Any]],  # [{product_id, product_name, demand, contribution_per_unit, factor_per_unit}]
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Analyze optimal production with limiting factor."""
    analysis = LimitingFactorAnalysis(
        analysis_date=datetime.utcnow(),
        limiting_factor=limiting_factor,
        factor_type=factor_type,
        total_available=total_available,
    )

    figures = calc.analyze(limiting_factor, factor_type, total_available, products)
    analysis.factor_used = figures["factor_used"]
    analysis.factor_remaining = figures["factor_remaining"]
    analysis.total_contribution = figures["total_contribution"]
    analysis.optimal_production_plan = figures["optimal_production_plan"]
    analysis.products = [ProductWithFactor(**p) for p in figures["products"]]

    return await crud.create(session, user_id, analysis)


@app.post("/compare-products")
async def compare_products_for_factor(
    products: List[Dict[str, Any]],  # [{product_id, product_name, contribution_per_unit, factor_per_unit}]
):
    """Compare products to determine ranking."""
    comparisons = calc.compare(products)

    return {"ranked_products": comparisons}


@app.get("/analyses")
async def list_analyses(
    limiting_factor: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List limiting factor analyses."""
    result = await crud.list_all(session, user_id, LimitingFactorAnalysis)
    if limiting_factor:
        result = [a for a in result if a.limiting_factor == limiting_factor]
    return {"analyses": result}


@app.get("/analyses/{analysis_id}")
async def get_analysis(
    analysis_id: str,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Get analysis details."""
    analysis = await crud.find(session, user_id, LimitingFactorAnalysis, analysis_id)
    if not analysis:
        return {"error": "Analysis not found"}
    return analysis


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
