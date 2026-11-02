"""
Vimbai Selling Price Reduction Decision Service
Analyzes proposed selling price reductions.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "selling_price_reduction_service" not in _sys.modules or not hasattr(
    _sys.modules.get("selling_price_reduction_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "selling_price_reduction_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["selling_price_reduction_service"] = _pkg
    _sys.modules["selling_price_reduction_service"].__path__ = [_HERE]

import os
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from selling_price_reduction_service import crud
from selling_price_reduction_service.dependencies import book_id_var, get_db_session, get_user_id
from selling_price_reduction_service.models import PriceReductionAnalysis

SERVICE_NAME = "selling-price-reduction-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8076"))
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

app = FastAPI(title="Vimbai Selling Price Reduction Service", version=SERVICE_VERSION, docs_url="/docs")
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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Selling price reduction analysis"}


@app.post("/analyze")
async def analyze_price_reduction(
    product_id: str,
    product_name: str,
    current_price: float,
    proposed_price: float,
    variable_cost_per_unit: float,
    current_expected_sales: float,
    new_expected_sales: Optional[float] = None,
    target_contribution: Optional[float] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Analyze selling price reduction decision."""
    analysis = PriceReductionAnalysis(
        product_id=product_id,
        product_name=product_name,
        current_price=current_price,
        proposed_price=proposed_price,
        current_expected_sales=current_expected_sales,
    )

    # Calculate price reduction
    analysis.price_reduction = current_price - proposed_price
    analysis.price_reduction_percentage = (analysis.price_reduction / current_price * 100) if current_price > 0 else 0

    # Calculate contributions
    analysis.current_contribution_per_unit = current_price - variable_cost_per_unit
    analysis.new_contribution_per_unit = proposed_price - variable_cost_per_unit
    analysis.contribution_change_per_unit = analysis.new_contribution_per_unit - analysis.current_contribution_per_unit

    # Calculate total contributions
    analysis.current_total_contribution = analysis.current_contribution_per_unit * current_expected_sales

    if new_expected_sales is not None:
        analysis.new_expected_sales = new_expected_sales
    else:
        # Assume same sales volume
        analysis.new_expected_sales = current_expected_sales

    analysis.new_total_contribution = analysis.new_contribution_per_unit * analysis.new_expected_sales
    analysis.contribution_difference = analysis.new_total_contribution - analysis.current_total_contribution

    # Calculate minimum price for target contribution
    if target_contribution and current_expected_sales > 0:
        required_contribution_per_unit = target_contribution / current_expected_sales
        analysis.minimum_price_for_target_contribution = variable_cost_per_unit + required_contribution_per_unit

    # Calculate break-even volume increase
    if abs(analysis.contribution_change_per_unit) > 0:
        # How much extra volume needed to maintain same total contribution
        analysis.break_even_volume_increase = (
            (analysis.current_total_contribution / analysis.new_contribution_per_unit - current_expected_sales)
            if analysis.new_contribution_per_unit > 0
            else float("inf")
        )

    # Make recommendation
    if analysis.contribution_difference > 0:
        analysis.recommendation = "APPROVE"
    elif analysis.contribution_difference < 0:
        if new_expected_sales and analysis.break_even_volume_increase > 0:
            analysis.recommendation = "REJECT"
        else:
            analysis.recommendation = "CONDITIONAL_APPROVAL"
    else:
        analysis.recommendation = "INDIFFERENT"

    return await crud.create(session, user_id, analysis)


@app.post("/discount-analysis")
async def analyze_discount_levels(
    product_name: str,
    list_price: float,
    variable_cost_per_unit: float,
    expected_sales_at_list: float,
    discounts: List[float],  # List of discount percentages to test
):
    """Analyze different discount levels."""
    results = []

    for discount_pct in discounts:
        selling_price = list_price * (1 - discount_pct / 100)
        contribution_per_unit = selling_price - variable_cost_per_unit

        # Estimate sales increase based on common demand elasticity
        # Assume 1% price reduction increases volume by 2% (adjustable)
        estimated_volume_multiplier = 1 + (discount_pct * 0.02)
        estimated_sales = expected_sales_at_list * estimated_volume_multiplier

        total_contribution = contribution_per_unit * estimated_sales
        original_contribution = (list_price - variable_cost_per_unit) * expected_sales_at_list
        contribution_diff = total_contribution - original_contribution

        results.append(
            {
                "discount_percentage": discount_pct,
                "selling_price": selling_price,
                "contribution_per_unit": contribution_per_unit,
                "estimated_sales_volume": estimated_sales,
                "total_contribution": total_contribution,
                "contribution_vs_original": contribution_diff,
                "recommended": contribution_diff > 0,
            }
        )

    return {"product_name": product_name, "discount_analysis": results}


@app.get("/analyses")
async def list_analyses(
    product_id: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List price reduction analyses."""
    result = await crud.list_all(session, user_id, PriceReductionAnalysis)
    if product_id:
        result = [a for a in result if a.product_id == product_id]
    return {"analyses": result}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
