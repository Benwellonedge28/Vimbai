"""
Vimbai Profit & Loss Account Service
Generates detailed P&L statements with expense categorization and margin analysis.
Port: 8336
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional, Any
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "profit-loss-account-service"
PORT = int(os.getenv("PORT", "8336"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Profit & Loss Account Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class PnLRequest(BaseModel):
    company_id: str; fiscal_year_start: str; fiscal_year_end: str
    revenue: float; cost_of_goods_sold: float = 0; operating_expenses: float = 0
    depreciation: float = 0; interest_expense: float = 0; tax_rate: float = 0.25
    other_income: float = 0; other_expenses: float = 0

class PnLStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; period_start: str; period_end: str
    revenue: float; other_income: float; total_revenue: float
    cost_of_goods_sold: float; gross_profit: float; gross_margin: float
    operating_expenses: float; depreciation: float; operating_income: float; operating_margin: float
    interest_expense: float; other_expenses: float; pretax_income: float
    tax_expense: float; net_income: float; net_margin: float
    eps: Optional[float] = None; ebitda: float; ebit: float

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/prepare", response_model=PnLStatement)
async def prepare_pnl(req: PnLRequest):
    total_revenue = req.revenue + req.other_income
    gross_profit = req.revenue - req.cost_of_goods_sold
    gross_margin = (gross_profit / req.revenue * 100) if req.revenue else 0
    ebitda = gross_profit - req.operating_expenses
    ebit = ebitda - req.depreciation
    operating_income = ebit
    operating_margin = (operating_income / req.revenue * 100) if req.revenue else 0
    pretax_income = operating_income - req.interest_expense - req.other_expenses
    tax_expense = max(pretax_income, 0) * req.tax_rate
    net_income = pretax_income - tax_expense
    net_margin = (net_income / req.revenue * 100) if req.revenue else 0
    
    return PnLStatement(
        company_id=req.company_id, period_start=req.fiscal_year_start, period_end=req.fiscal_year_end,
        revenue=req.revenue, other_income=req.other_income, total_revenue=total_revenue,
        cost_of_goods_sold=req.cost_of_goods_sold, gross_profit=round(gross_profit, 2),
        gross_margin=round(gross_margin, 2), operating_expenses=req.operating_expenses,
        depreciation=req.depreciation, operating_income=round(operating_income, 2),
        operating_margin=round(operating_margin, 2), interest_expense=req.interest_expense,
        other_expenses=req.other_expenses, pretax_income=round(pretax_income, 2),
        tax_expense=round(tax_expense, 2), net_income=round(net_income, 2),
        net_margin=round(net_margin, 2), ebitda=round(ebitda, 2), ebit=round(ebit, 2)
    )

@app.post("/compare", response_model=dict)
async def compare_periods(company_id: str, current: PnLRequest, previous_revenue: float, previous_net_income: float):
    total_revenue = current.revenue + current.other_income
    cogs = current.cost_of_goods_sold
    gross_profit = current.revenue - cogs
    operating_income = gross_profit - current.operating_expenses - current.depreciation
    pretax = operating_income - current.interest_expense - current.other_expenses
    tax = max(pretax, 0) * current.tax_rate
    net_income = pretax - tax
    
    revenue_change = ((current.revenue - previous_revenue) / previous_revenue * 100) if previous_revenue else 0
    income_change = ((net_income - previous_net_income) / abs(previous_net_income) * 100) if previous_net_income else 0
    
    return {
        "company_id": company_id,
        "revenue_change_pct": round(revenue_change, 2),
        "net_income_change_pct": round(income_change, 2),
        "current_revenue": current.revenue, "previous_revenue": previous_revenue,
        "current_net_income": round(net_income, 2), "previous_net_income": previous_net_income
    }

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
