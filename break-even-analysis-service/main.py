"""
FinAcc Break-Even Analysis Service
Comprehensive break-even analysis for products and businesses.
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

SERVICE_NAME = "break-even-analysis-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8078"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Break-Even Analysis Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class BreakEvenAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_id: str
    entity_name: str
    analysis_date: datetime

    # Input Data
    selling_price_per_unit: float
    variable_cost_per_unit: float
    fixed_costs: float

    # Calculated Values
    contribution_per_unit: float = 0
    contribution_margin_ratio: float = 0
    break_even_units: float = 0
    break_even_revenue: float = 0

    # Target Profit Analysis
    target_profit: float = 0
    units_for_target_profit: float = 0
    revenue_for_target_profit: float = 0

    # Margin of Safety
    expected_sales_units: float = 0
    expected_sales_revenue: float = 0
    margin_of_safety_units: float = 0
    margin_of_safety_revenue: float = 0
    margin_of_safety_percentage: float = 0

    # Additional Metrics
    profit_at_expected_sales: float = 0
    degree_of_operating_leverage: float = 0

    created_at: datetime = Field(default_factory=datetime.utcnow)


analyses: List[BreakEvenAnalysis] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Break-even analysis"}


@app.post("/analyze")
async def perform_break_even_analysis(
    entity_id: str, entity_name: str,
    selling_price_per_unit: float, variable_cost_per_unit: float,
    fixed_costs: float, expected_sales_units: float = 0,
    target_profit: float = 0
):
    """Perform comprehensive break-even analysis."""
    analysis = BreakEvenAnalysis(
        entity_id=entity_id, entity_name=entity_name,
        analysis_date=datetime.utcnow(),
        selling_price_per_unit=selling_price_per_unit,
        variable_cost_per_unit=variable_cost_per_unit,
        fixed_costs=fixed_costs,
        expected_sales_units=expected_sales_units,
        target_profit=target_profit
    )

    # Calculate contribution
    analysis.contribution_per_unit = selling_price_per_unit - variable_cost_per_unit

    # Calculate contribution margin ratio
    if selling_price_per_unit > 0:
        analysis.contribution_margin_ratio = (
            analysis.contribution_per_unit / selling_price_per_unit * 100
        )

    # Calculate break-even point
    if analysis.contribution_per_unit > 0:
        analysis.break_even_units = fixed_costs / analysis.contribution_per_unit
    analysis.break_even_revenue = analysis.break_even_units * selling_price_per_unit

    # Calculate units for target profit
    if analysis.contribution_per_unit > 0:
        analysis.units_for_target_profit = (fixed_costs + target_profit) / analysis.contribution_per_unit
    analysis.revenue_for_target_profit = analysis.units_for_target_profit * selling_price_per_unit

    # Calculate expected sales revenue
    analysis.expected_sales_revenue = expected_sales_units * selling_price_per_unit

    # Calculate margin of safety
    if expected_sales_units > 0:
        analysis.margin_of_safety_units = expected_sales_units - analysis.break_even_units
        analysis.margin_of_safety_revenue = analysis.margin_of_safety_units * selling_price_per_unit
        if analysis.expected_sales_revenue > 0:
            analysis.margin_of_safety_percentage = (
                analysis.margin_of_safety_revenue / analysis.expected_sales_revenue * 100
            )

    # Calculate profit at expected sales
    analysis.profit_at_expected_sales = (
        expected_sales_units * analysis.contribution_per_unit - fixed_costs
    )

    # Calculate degree of operating leverage
    if analysis.profit_at_expected_sales != 0:
        contribution = expected_sales_units * analysis.contribution_per_unit
        analysis.degree_of_operating_leverage = contribution / analysis.profit_at_expected_sales

    analyses.append(analysis)
    return analysis


@app.post("/quick-analysis")
async def quick_break_even_analysis(
    fixed_costs: float, selling_price: float, variable_cost: float
):
    """Quick break-even calculation."""
    contribution = selling_price - variable_cost
    contribution_ratio = (contribution / selling_price * 100) if selling_price > 0 else 0
    break_even_units = fixed_costs / contribution if contribution > 0 else float('inf')
    break_even_revenue = break_even_units * selling_price

    return {
        "fixed_costs": fixed_costs,
        "selling_price": selling_price,
        "variable_cost": variable_cost,
        "contribution_per_unit": contribution,
        "contribution_margin_ratio": contribution_ratio,
        "break_even_units": break_even_units,
        "break_even_revenue": break_even_revenue
    }


@app.get("/analyses")
async def list_analyses(entity_id: Optional[str] = None):
    """List break-even analyses."""
    result = analyses
    if entity_id:
        result = [a for a in result if a.entity_id == entity_id]
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