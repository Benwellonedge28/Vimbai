"""
FinAcc Break-Even Revenue Service
Calculates break-even revenue/target revenue.
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

SERVICE_NAME = "break-even-revenue-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8080"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Break-Even Revenue Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class BreakEvenRevenue(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_id: str
    entity_name: str
    period: str
    fixed_costs: float
    total_expected_revenue: float
    contribution_margin_ratio: float
    break_even_revenue: float = 0
    target_revenue_for_profit: float = 0
    target_profit: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


class RevenueAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_name: str
    analysis_type: str  # break_even, target_profit, target_margin
    revenue_calculated: float
    fixed_costs: float
    contribution_margin_ratio: float
    assumptions: List[str] = []
    created_at: datetime = Field(default_factory=datetime.utcnow)


revenues: List[BreakEvenRevenue] = []
revenue_analyses: List[RevenueAnalysis] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Break-even revenue calculation"}


@app.post("/calculate")
async def calculate_break_even_revenue(
    entity_id: str, entity_name: str, period: str,
    fixed_costs: float, contribution_margin_ratio: float,
    total_expected_revenue: float = 0, target_profit: float = 0
):
    """Calculate break-even revenue."""
    revenue = BreakEvenRevenue(
        entity_id=entity_id, entity_name=entity_name, period=period,
        fixed_costs=fixed_costs, contribution_margin_ratio=contribution_margin_ratio,
        total_expected_revenue=total_expected_revenue, target_profit=target_profit
    )

    # Calculate break-even revenue
    # Formula: Fixed Costs / Contribution Margin Ratio
    if contribution_margin_ratio > 0:
        revenue.break_even_revenue = fixed_costs / contribution_margin_ratio

    # Calculate target revenue for profit
    if target_profit > 0 and contribution_margin_ratio > 0:
        revenue.target_revenue_for_profit = (fixed_costs + target_profit) / contribution_margin_ratio

    revenues.append(revenue)
    return revenue


@app.post("/target-revenue")
async def calculate_target_revenue(
    entity_name: str, fixed_costs: float,
    contribution_margin_ratio: float, desired_profit: float,
    desired_return_on_sales: Optional[float] = None
):
    """Calculate target revenue for desired profit."""
    # Formula: (Fixed Costs + Desired Profit) / Contribution Margin Ratio
    target_revenue = (fixed_costs + desired_profit) / contribution_margin_ratio if contribution_margin_ratio > 0 else 0

    analysis = RevenueAnalysis(
        entity_name=entity_name, analysis_type="target_profit",
        revenue_calculated=target_revenue,
        fixed_costs=fixed_costs, contribution_margin_ratio=contribution_margin_ratio,
        assumptions=[f"Desired profit: {desired_profit}"]
    )

    # If return on sales is specified, calculate required revenue
    if desired_return_on_sales:
        required_revenue = desired_profit / (desired_return_on_sales / 100) if desired_return_on_sales > 0 else 0
        analysis.assumptions.append(f"Desired return on sales: {desired_return_on_sales}%")
        analysis.assumptions.append(f"Required revenue for this return: {required_revenue}")

    revenue_analyses.append(analysis)
    return analysis


@app.post("/required-sales-mix")
async def calculate_required_sales_mix(
    entity_name: str, fixed_costs: float,
    products: List[Dict[str, Any]]  # [{product_name, selling_price, variable_cost, expected_proportion}]
):
    """Calculate required revenue from each product."""
    results = []
    total_cm_ratio = 0

    # Calculate weighted average contribution margin ratio
    for prod in products:
        price = prod["selling_price"]
        var_cost = prod["variable_cost"]
        proportion = prod.get("expected_proportion", 0)
        cm = price - var_cost
        cm_ratio = cm / price if price > 0 else 0
        total_cm_ratio += cm_ratio * (proportion / 100)

    # Calculate break-even revenue
    break_even_revenue = fixed_costs / total_cm_ratio if total_cm_ratio > 0 else 0

    for prod in products:
        prop = prod.get("expected_proportion", 0)
        results.append({
            "product_name": prod["product_name"],
            "proportion": prop,
            "required_revenue": break_even_revenue * (prop / 100),
            "selling_price": prod["selling_price"],
            "variable_cost": prod["variable_cost"],
            "contribution": prod["selling_price"] - prod["variable_cost"]
        })

    return {
        "entity_name": entity_name,
        "total_break_even_revenue": break_even_revenue,
        "weighted_cm_ratio": total_cm_ratio,
        "product_breakdown": results
    }


@app.get("/revenues")
async def list_revenues(entity_id: Optional[str] = None):
    """List break-even revenues."""
    result = revenues
    if entity_id:
        result = [r for r in result if r.entity_id == entity_id]
    return {"revenues": result}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)