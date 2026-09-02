"""
Vimbai CVP Analysis Service (Merged)
Port: 8078

This service consolidates the following former services:
  - break-even-analysis-service (Port: 8078)
  - break-even-point-service (Port: 8079)
  - break-even-revenue-service (Port: 8080)
  - break-even-output-service (Port: 8081)
  - contribution-per-unit-service (Port: 8082)
  - contribution-analysis-service (Port: 8083)

Capabilities:
  - Comprehensive Cost-Volume-Profit (CVP) analysis
  - Break-even point calculation (units and revenue)
  - Contribution margin per unit and ratio
  - Target profit analysis (required units and revenue)
  - Margin of safety (units, revenue, percentage)
  - Degree of operating leverage
  - Multi-product break-even and sales mix analysis
  - Quick CVP calculations for ad-hoc queries
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

# ============================================================================
# Configuration
# ============================================================================

SERVICE_NAME = "cvp-analysis-service"
SERVICE_VERSION = "2.0.0"
PORT = int(os.getenv("PORT", "8078"))

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

app = FastAPI(
    title="Vimbai CVP Analysis Service",
    description="Consolidated Cost-Volume-Profit, Break-Even, and Contribution Margin Analysis",
    version=SERVICE_VERSION,
    docs_url="/docs",
)
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

# ============================================================================
# Pydantic Models
# ============================================================================


class CVPAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_id: str
    entity_name: str
    analysis_date: datetime

    # Inputs
    selling_price_per_unit: float
    variable_cost_per_unit: float
    fixed_costs: float
    expected_sales_units: float = 0.0
    target_profit: float = 0.0

    # Contribution
    contribution_per_unit: float = 0.0
    contribution_margin_ratio: float = 0.0

    # Break-even
    break_even_units: float = 0.0
    break_even_revenue: float = 0.0

    # Target profit
    units_for_target_profit: float = 0.0
    revenue_for_target_profit: float = 0.0

    # Margin of safety
    expected_sales_revenue: float = 0.0
    margin_of_safety_units: float = 0.0
    margin_of_safety_revenue: float = 0.0
    margin_of_safety_percentage: float = 0.0

    # Additional metrics
    profit_at_expected_sales: float = 0.0
    degree_of_operating_leverage: float = 0.0

    created_at: datetime = Field(default_factory=datetime.utcnow)


class ContributionRequest(BaseModel):
    entity_id: str
    entity_name: str
    selling_price_per_unit: float
    variable_cost_per_unit: float
    units_sold: float = 0.0


class MultiProductItem(BaseModel):
    product_name: str
    selling_price: float
    variable_cost: float
    expected_proportion: float  # percentage of total sales mix


class MultiProductRequest(BaseModel):
    entity_id: str
    entity_name: str
    fixed_costs: float
    products: List[MultiProductItem]
    target_profit: float = 0.0


# ============================================================================
# In-Memory Storage
# ============================================================================

analyses: List[CVPAnalysis] = []

# ============================================================================
# Helper
# ============================================================================


def _perform_cvp(
    entity_id: str,
    entity_name: str,
    selling_price_per_unit: float,
    variable_cost_per_unit: float,
    fixed_costs: float,
    expected_sales_units: float = 0.0,
    target_profit: float = 0.0,
) -> CVPAnalysis:
    analysis = CVPAnalysis(
        entity_id=entity_id,
        entity_name=entity_name,
        analysis_date=datetime.utcnow(),
        selling_price_per_unit=selling_price_per_unit,
        variable_cost_per_unit=variable_cost_per_unit,
        fixed_costs=fixed_costs,
        expected_sales_units=expected_sales_units,
        target_profit=target_profit,
    )

    # Contribution
    analysis.contribution_per_unit = selling_price_per_unit - variable_cost_per_unit
    if selling_price_per_unit > 0:
        analysis.contribution_margin_ratio = analysis.contribution_per_unit / selling_price_per_unit

    # Break-even
    if analysis.contribution_per_unit > 0:
        analysis.break_even_units = fixed_costs / analysis.contribution_per_unit
    analysis.break_even_revenue = analysis.break_even_units * selling_price_per_unit

    # Target profit
    if analysis.contribution_per_unit > 0:
        analysis.units_for_target_profit = (fixed_costs + target_profit) / analysis.contribution_per_unit
    analysis.revenue_for_target_profit = analysis.units_for_target_profit * selling_price_per_unit

    # Expected sales
    analysis.expected_sales_revenue = expected_sales_units * selling_price_per_unit

    # Margin of safety
    if expected_sales_units > 0:
        analysis.margin_of_safety_units = expected_sales_units - analysis.break_even_units
        analysis.margin_of_safety_revenue = analysis.margin_of_safety_units * selling_price_per_unit
        if analysis.expected_sales_revenue > 0:
            analysis.margin_of_safety_percentage = (
                analysis.margin_of_safety_revenue / analysis.expected_sales_revenue * 100
            )

    # Profit at expected sales
    analysis.profit_at_expected_sales = expected_sales_units * analysis.contribution_per_unit - fixed_costs

    # Degree of operating leverage
    if analysis.profit_at_expected_sales != 0:
        contribution_total = expected_sales_units * analysis.contribution_per_unit
        analysis.degree_of_operating_leverage = contribution_total / analysis.profit_at_expected_sales

    return analysis


# ============================================================================
# Routes — Health
# ============================================================================


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "description": "CVP, Break-Even, and Contribution Margin Analysis",
    }


# ============================================================================
# Routes — Full CVP Analysis
# ============================================================================


@app.post("/analyze")
async def perform_cvp_analysis(
    entity_id: str,
    entity_name: str,
    selling_price_per_unit: float,
    variable_cost_per_unit: float,
    fixed_costs: float,
    expected_sales_units: float = 0.0,
    target_profit: float = 0.0,
):
    """Perform a comprehensive CVP analysis including break-even, target profit, margin of safety, and operating leverage."""
    logger.info("Performing CVP analysis", entity=entity_id)
    analysis = _perform_cvp(
        entity_id,
        entity_name,
        selling_price_per_unit,
        variable_cost_per_unit,
        fixed_costs,
        expected_sales_units,
        target_profit,
    )
    analyses.append(analysis)
    return analysis


@app.post("/quick-analysis")
async def quick_cvp_analysis(
    fixed_costs: float,
    selling_price: float,
    variable_cost: float,
):
    """Quick CVP calculation without persistence."""
    contribution = selling_price - variable_cost
    contribution_ratio = (contribution / selling_price) if selling_price > 0 else 0.0
    break_even_units = fixed_costs / contribution if contribution > 0 else float("inf")
    break_even_revenue = break_even_units * selling_price if contribution > 0 else float("inf")

    return {
        "fixed_costs": fixed_costs,
        "selling_price": selling_price,
        "variable_cost": variable_cost,
        "contribution_per_unit": round(contribution, 4),
        "contribution_margin_ratio": round(contribution_ratio, 4),
        "break_even_units": round(break_even_units, 2),
        "break_even_revenue": round(break_even_revenue, 2),
    }


# ============================================================================
# Routes — Contribution Margin
# ============================================================================


@app.post("/contribution")
async def calculate_contribution(request: ContributionRequest):
    """Calculate contribution margin per unit, ratio, and total contribution."""
    logger.info("Calculating contribution margin", entity=request.entity_id)

    contribution_per_unit = request.selling_price_per_unit - request.variable_cost_per_unit
    contribution_margin_ratio = (
        contribution_per_unit / request.selling_price_per_unit if request.selling_price_per_unit > 0 else 0.0
    )
    total_contribution = contribution_per_unit * request.units_sold

    return {
        "entity_id": request.entity_id,
        "entity_name": request.entity_name,
        "selling_price_per_unit": request.selling_price_per_unit,
        "variable_cost_per_unit": request.variable_cost_per_unit,
        "contribution_per_unit": round(contribution_per_unit, 4),
        "contribution_margin_ratio": round(contribution_margin_ratio, 4),
        "contribution_margin_percentage": round(contribution_margin_ratio * 100, 2),
        "units_sold": request.units_sold,
        "total_contribution": round(total_contribution, 2),
    }


# ============================================================================
# Routes — Target Profit
# ============================================================================


@app.post("/target-profit")
async def calculate_target_profit(
    entity_name: str,
    selling_price_per_unit: float,
    variable_cost_per_unit: float,
    fixed_costs: float,
    target_profit: float,
    desired_return_on_sales: Optional[float] = None,
):
    """Calculate required units and revenue to achieve a target profit."""
    contribution = selling_price_per_unit - variable_cost_per_unit
    cm_ratio = (contribution / selling_price_per_unit) if selling_price_per_unit > 0 else 0.0

    if contribution <= 0:
        raise HTTPException(status_code=400, detail="Contribution per unit must be positive.")

    required_units = (fixed_costs + target_profit) / contribution
    required_revenue = required_units * selling_price_per_unit

    result = {
        "entity_name": entity_name,
        "target_profit": target_profit,
        "contribution_per_unit": round(contribution, 4),
        "contribution_margin_ratio": round(cm_ratio, 4),
        "required_units": round(required_units, 2),
        "required_revenue": round(required_revenue, 2),
    }

    if desired_return_on_sales is not None and desired_return_on_sales > 0:
        revenue_for_ros = target_profit / (desired_return_on_sales / 100)
        result["desired_return_on_sales_pct"] = desired_return_on_sales
        result["required_revenue_for_ros"] = round(revenue_for_ros, 2)

    return result


# ============================================================================
# Routes — Multi-Product Analysis
# ============================================================================


@app.post("/multi-product")
async def multi_product_analysis(request: MultiProductRequest):
    """Calculate break-even and target profit for a multi-product sales mix."""
    logger.info("Multi-product CVP analysis", entity=request.entity_id)

    if not request.products:
        raise HTTPException(status_code=400, detail="At least one product is required.")

    # Weighted average contribution margin ratio
    weighted_cm_ratio = 0.0
    product_details = []
    for prod in request.products:
        cm = prod.selling_price - prod.variable_cost
        cm_ratio = cm / prod.selling_price if prod.selling_price > 0 else 0.0
        weight = prod.expected_proportion / 100
        weighted_cm_ratio += cm_ratio * weight
        product_details.append(
            {
                "product_name": prod.product_name,
                "selling_price": prod.selling_price,
                "variable_cost": prod.variable_cost,
                "contribution_per_unit": round(cm, 4),
                "contribution_margin_ratio": round(cm_ratio, 4),
                "expected_proportion_pct": prod.expected_proportion,
            }
        )

    break_even_revenue = request.fixed_costs / weighted_cm_ratio if weighted_cm_ratio > 0 else float("inf")
    target_revenue = (
        (request.fixed_costs + request.target_profit) / weighted_cm_ratio if weighted_cm_ratio > 0 else float("inf")
    )

    # Allocate break-even revenue by product mix
    for prod_detail in product_details:
        proportion = prod_detail["expected_proportion_pct"] / 100
        prod_detail["break_even_revenue_share"] = round(break_even_revenue * proportion, 2)
        prod_detail["target_revenue_share"] = round(target_revenue * proportion, 2)

    return {
        "entity_id": request.entity_id,
        "entity_name": request.entity_name,
        "fixed_costs": request.fixed_costs,
        "target_profit": request.target_profit,
        "weighted_contribution_margin_ratio": round(weighted_cm_ratio, 4),
        "break_even_revenue": round(break_even_revenue, 2),
        "target_revenue": round(target_revenue, 2),
        "product_breakdown": product_details,
    }


# ============================================================================
# Routes — History
# ============================================================================


@app.get("/analyses")
async def list_analyses(entity_id: Optional[str] = None):
    """List stored CVP analyses."""
    result = analyses
    if entity_id:
        result = [a for a in result if a.entity_id == entity_id]
    return {"total": len(result), "analyses": result}


@app.get("/analyses/{analysis_id}")
async def get_analysis(analysis_id: str):
    """Get a specific CVP analysis by ID."""
    analysis = next((a for a in analyses if a.id == analysis_id), None)
    if not analysis:
        raise HTTPException(status_code=404, detail="Analysis not found")
    return analysis


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
