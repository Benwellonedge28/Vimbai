"""
FinAcc Contribution Analysis Service
Analyzes contribution for various business decisions.
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

SERVICE_NAME = "contribution-analysis-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8072"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Contribution Analysis Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class ProductContribution(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    product_name: str
    selling_price: float
    variable_cost_per_unit: float
    contribution_per_unit: float = 0
    contribution_margin_ratio: float = 0
    sales_mix_percentage: float = 0
    weighted_contribution: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ContributionStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    total_sales: float
    total_variable_costs: float
    total_contribution: float = 0
    total_fixed_costs: float
    profit: float = 0
    product_contributions: List[Dict[str, Any]] = []
    created_at: datetime = Field(default_factory=datetime.utcnow)


class SalesMixAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    analysis_date: datetime
    products: List[Dict[str, Any]] = []
    total_sales: float = 0
    total_contribution: float = 0
    average_contribution_margin: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


product_contributions: List[ProductContribution] = []
contribution_statements: List[ContributionStatement] = []
sales_mix_analyses: List[SalesMixAnalysis] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Contribution analysis"}


@app.post("/product/analyze")
async def analyze_product_contribution(
    product_id: str, product_name: str,
    selling_price: float, variable_cost_per_unit: float
):
    """Analyze contribution for a single product."""
    contribution = ProductContribution(
        product_id=product_id, product_name=product_name,
        selling_price=selling_price, variable_cost_per_unit=variable_cost_per_unit
    )

    contribution.contribution_per_unit = selling_price - variable_cost_per_unit
    if selling_price > 0:
        contribution.contribution_margin_ratio = (
            contribution.contribution_per_unit / selling_price * 100
        )

    product_contributions.append(contribution)
    return contribution


@app.post("/statement/generate")
async def generate_contribution_statement(
    company_id: str, period: str, total_sales: float,
    total_variable_costs: float, total_fixed_costs: float,
    product_sales: List[Dict[str, Any]]  # [{product_id, product_name, sales, variable_costs}]
):
    """Generate full contribution income statement."""
    statement = ContributionStatement(
        company_id=company_id, period=period,
        total_sales=total_sales,
        total_variable_costs=total_variable_costs,
        total_fixed_costs=total_fixed_costs
    )

    statement.total_contribution = total_sales - total_variable_costs
    statement.profit = statement.total_contribution - total_fixed_costs

    for prod in product_sales:
        prod_contribution = prod["sales"] - prod.get("variable_costs", 0)
        contribution_ratio = (prod_contribution / prod["sales"] * 100) if prod["sales"] > 0 else 0
        sales_mix = (prod["sales"] / total_sales * 100) if total_sales > 0 else 0

        statement.product_contributions.append({
            "product_id": prod["product_id"],
            "product_name": prod["product_name"],
            "sales": prod["sales"],
            "variable_costs": prod.get("variable_costs", 0),
            "contribution": prod_contribution,
            "contribution_margin_ratio": contribution_ratio,
            "sales_mix_percentage": sales_mix
        })

    contribution_statements.append(statement)
    return statement


@app.post("/sales-mix/analyze")
async def analyze_sales_mix(
    company_id: str,
    products: List[Dict[str, Any]]  # [{product_id, product_name, selling_price, variable_cost, units_sold}]
):
    """Analyze sales mix and weighted contribution."""
    analysis = SalesMixAnalysis(
        company_id=company_id, analysis_date=datetime.utcnow()
    )

    for prod in products:
        selling_price = prod["selling_price"]
        variable_cost = prod["variable_cost"]
        units = prod["units_sold"]

        sales = selling_price * units
        variable_costs = variable_cost * units
        contribution = sales - variable_costs
        contribution_ratio = (contribution / sales * 100) if sales > 0 else 0
        sales_mix = (sales / analysis.total_sales * 100) if analysis.total_sales > 0 else 0

        analysis.products.append({
            "product_id": prod["product_id"],
            "product_name": prod["product_name"],
            "units_sold": units,
            "selling_price": selling_price,
            "variable_cost": variable_cost,
            "sales": sales,
            "variable_costs": variable_costs,
            "contribution": contribution,
            "contribution_margin_ratio": contribution_ratio,
            "sales_mix_percentage": sales_mix
        })

        analysis.total_sales += sales
        analysis.total_contribution += contribution

    if analysis.total_sales > 0:
        analysis.average_contribution_margin = (
            analysis.total_contribution / analysis.total_sales * 100
        )

    sales_mix_analyses.append(analysis)
    return analysis


@app.post("/target-profit")
async def calculate_units_for_target_profit(
    contribution_per_unit: float, fixed_costs: float, target_profit: float
):
    """Calculate units needed to achieve target profit."""
    units_required = (fixed_costs + target_profit) / contribution_per_unit if contribution_per_unit > 0 else 0
    revenue_required = units_required * (contribution_per_unit + (fixed_costs / units_required if units_required > 0 else 0))

    return {
        "contribution_per_unit": contribution_per_unit,
        "fixed_costs": fixed_costs,
        "target_profit": target_profit,
        "units_required": units_required,
        "revenue_required": revenue_required
    }


@app.get("/products")
async def list_product_contributions(product_id: Optional[str] = None):
    """List product contributions."""
    result = product_contributions
    if product_id:
        result = [p for p in result if p.product_id == product_id]
    return {"products": result}


@app.get("/statements")
async def list_contribution_statements(
    company_id: Optional[str] = None,
    period: Optional[str] = None
):
    """List contribution statements."""
    result = contribution_statements
    if company_id:
        result = [s for s in result if s.company_id == company_id]
    if period:
        result = [s for s in result if s.period == period]
    return {"statements": result}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)