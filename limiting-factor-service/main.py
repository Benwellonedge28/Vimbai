"""
Vimbai Limiting Factor Service
Optimizes production when resources are scarce.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
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


class LimitingFactorType(str):
    MACHINE_HOURS = "machine_hours"
    LABOR_HOURS = "labor_hours"
    MATERIAL_KG = "material_kg"
    MATERIAL_UNITS = "material_units"
    SPACE = "space"
    BUDGET = "budget"


class ProductWithFactor(BaseModel):
    product_id: str
    product_name: str
    demand: float
    contribution_per_unit: float
    factor_per_unit: float  # Resource usage per unit
    contribution_per_factor_unit: float = 0
    ranking: int = 0
    units_to_produce: float = 0
    total_contribution: float = 0


class LimitingFactorAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    analysis_date: datetime
    limiting_factor: str
    factor_type: str
    total_available: float
    factor_used: float = 0
    factor_remaining: float = 0
    products: List[ProductWithFactor] = []
    total_contribution: float = 0
    optimal_production_plan: Dict[str, float] = {}  # product_id -> units
    created_at: datetime = Field(default_factory=datetime.utcnow)


analyses: List[LimitingFactorAnalysis] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Limiting factor analysis"}


@app.post("/analyze")
async def analyze_limiting_factor(
    limiting_factor: str,
    factor_type: str,
    total_available: float,
    products: List[Dict[str, Any]],  # [{product_id, product_name, demand, contribution_per_unit, factor_per_unit}]
):
    """Analyze optimal production with limiting factor."""
    analysis = LimitingFactorAnalysis(
        analysis_date=datetime.utcnow(),
        limiting_factor=limiting_factor,
        factor_type=factor_type,
        total_available=total_available,
    )

    # Calculate contribution per factor unit for each product
    for prod in products:
        product = ProductWithFactor(
            product_id=prod["product_id"],
            product_name=prod["product_name"],
            demand=prod["demand"],
            contribution_per_unit=prod["contribution_per_unit"],
            factor_per_unit=prod["factor_per_unit"],
        )

        if prod["factor_per_unit"] > 0:
            product.contribution_per_factor_unit = prod["contribution_per_unit"] / prod["factor_per_unit"]

        analysis.products.append(product)

    # Sort by contribution per factor unit (highest first)
    analysis.products.sort(key=lambda x: x.contribution_per_factor_unit, reverse=True)

    # Rank products
    for i, product in enumerate(analysis.products, 1):
        product.ranking = i

    # Allocate resources optimally
    remaining_factor = total_available

    for product in analysis.products:
        # Calculate max units possible
        if product.factor_per_unit > 0:
            max_units_by_factor = remaining_factor / product.factor_per_unit
            product.units_to_produce = min(product.demand, max_units_by_factor)
        else:
            product.units_to_produce = product.demand

        # Update remaining factor
        factor_used = product.units_to_produce * product.factor_per_unit
        remaining_factor -= factor_used
        analysis.factor_used += factor_used

        # Calculate contribution
        product.total_contribution = product.units_to_produce * product.contribution_per_unit
        analysis.total_contribution += product.total_contribution

        # Add to optimal plan
        analysis.optimal_production_plan[product.product_id] = product.units_to_produce

    analysis.factor_remaining = remaining_factor
    analyses.append(analysis)
    return analysis


@app.post("/compare-products")
async def compare_products_for_factor(
    products: List[Dict[str, Any]],  # [{product_id, product_name, contribution_per_unit, factor_per_unit}]
):
    """Compare products to determine ranking."""
    comparisons = []

    for prod in products:
        contribution_per_factor = 0
        if prod["factor_per_unit"] > 0:
            contribution_per_factor = prod["contribution_per_unit"] / prod["factor_per_unit"]

        comparisons.append(
            {
                "product_id": prod["product_id"],
                "product_name": prod["product_name"],
                "contribution_per_unit": prod["contribution_per_unit"],
                "factor_per_unit": prod["factor_per_unit"],
                "contribution_per_factor_unit": contribution_per_factor,
            }
        )

    # Sort by contribution per factor unit
    comparisons.sort(key=lambda x: x["contribution_per_factor_unit"], reverse=True)

    # Add rankings
    for i, comp in enumerate(comparisons, 1):
        comp["ranking"] = i

    return {"ranked_products": comparisons}


@app.get("/analyses")
async def list_analyses(limiting_factor: Optional[str] = None):
    """List limiting factor analyses."""
    result = analyses
    if limiting_factor:
        result = [a for a in result if a.limiting_factor == limiting_factor]
    return {"analyses": result}


@app.get("/analyses/{analysis_id}")
async def get_analysis(analysis_id: str):
    """Get analysis details."""
    analysis = next((a for a in analyses if a.id == analysis_id), None)
    if not analysis:
        return {"error": "Analysis not found"}
    return analysis


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
