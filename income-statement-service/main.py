"""
Vimbai Income Statement Service
Full income statement preparation with multi-period comparison and EPS calculation.
Port: 8334
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "income-statement-service"
PORT = int(os.getenv("PORT", "8334"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Income Statement Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class IncomeStatementRequest(BaseModel):
    company_id: str; fiscal_year: int = 2026
    revenue: float; cost_of_goods_sold: float = 0; operating_expenses: float = 0
    depreciation: float = 0; amortization: float = 0; interest_expense: float = 0
    tax_rate: float = 0.25; other_income: float = 0; other_expenses: float = 0
    shares_outstanding: int = 0; diluted_shares: int = 0

class IncomeStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; fiscal_year: int
    revenue: float; other_income: float; total_revenue: float
    cogs: float; gross_profit: float; gross_margin: float
    operating_expenses: float; depreciation_amortization: float
    operating_income: float; operating_margin: float
    interest_expense: float; other_expenses: float
    pretax_income: float; tax_expense: float; net_income: float
    net_margin: float; eps: Optional[float] = None; diluted_eps: Optional[float] = None
    ebitda: float; ebit: float

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/prepare", response_model=IncomeStatement)
async def prepare_income_statement(req: IncomeStatementRequest):
    total_revenue = req.revenue + req.other_income
    gross_profit = req.revenue - req.cost_of_goods_sold
    gross_margin = (gross_profit / req.revenue * 100) if req.revenue else 0
    da = req.depreciation + req.amortization
    operating_income = gross_profit - req.operating_expenses - da
    operating_margin = (operating_income / req.revenue * 100) if req.revenue else 0
    ebitda = gross_profit - req.operating_expenses
    ebit = operating_income
    pretax = operating_income - req.interest_expense - req.other_expenses
    tax = max(pretax, 0) * req.tax_rate
    net_income = pretax - tax
    net_margin = (net_income / req.revenue * 100) if req.revenue else 0
    
    eps = round(net_income / req.shares_outstanding, 2) if req.shares_outstanding else None
    diluted = round(net_income / req.diluted_shares, 2) if req.diluted_shares else None
    
    return IncomeStatement(
        company_id=req.company_id, fiscal_year=req.fiscal_year,
        revenue=round(req.revenue, 2), other_income=round(req.other_income, 2),
        total_revenue=round(total_revenue, 2),
        cogs=round(req.cost_of_goods_sold, 2),
        gross_profit=round(gross_profit, 2), gross_margin=round(gross_margin, 2),
        operating_expenses=round(req.operating_expenses, 2),
        depreciation_amortization=round(da, 2),
        operating_income=round(operating_income, 2), operating_margin=round(operating_margin, 2),
        interest_expense=round(req.interest_expense, 2), other_expenses=round(req.other_expenses, 2),
        pretax_income=round(pretax, 2), tax_expense=round(tax, 2),
        net_income=round(net_income, 2), net_margin=round(net_margin, 2),
        eps=eps, diluted_eps=diluted,
        ebitda=round(ebitda, 2), ebit=round(ebit, 2)
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
