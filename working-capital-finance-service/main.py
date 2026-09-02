"""
Vimbai Working Capital Finance Service
Working capital optimization, factoring, and short-term financing analysis.
Port: 8381
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "working-capital-finance-service"
PORT = int(os.getenv("PORT", "8381"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Working Capital Finance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class WorkingCapitalRequest(BaseModel):
    company_id: str; period: str
    current_assets: float; current_liabilities: float
    inventory: float = 0; accounts_receivable: float = 0; accounts_payable: float = 0
    annual_revenue: float = 0; cogs: float = 0

class WorkingCapitalResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; period: str
    working_capital: float; current_ratio: float; quick_ratio: float
    cash_conversion_cycle: float; dso: float; dio: float; dpo: float
    financing_need: float; recommendation: str

class FactoringRequest(BaseModel):
    company_id: str; invoice_amount: float; advance_rate: float = 0.85
    factor_fee_rate: float = 0.03; discount_rate: float = 0.02

class FactoringResult(BaseModel):
    invoice_amount: float; advance_amount: float; fee: float
    discount: float; net_to_company: float; total_cost: float
    effective_annual_rate: float

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/analyze", response_model=WorkingCapitalResult)
async def analyze_working_capital(req: WorkingCapitalRequest):
    wc = req.current_assets - req.current_liabilities
    current_ratio = req.current_assets / req.current_liabilities if req.current_liabilities else 0
    quick_ratio = (req.current_assets - req.inventory) / req.current_liabilities if req.current_liabilities else 0
    
    dso = (req.accounts_receivable / req.annual_revenue * 365) if req.annual_revenue else 0
    dio = (req.inventory / req.cogs * 365) if req.cogs else 0
    dpo = (req.accounts_payable / req.cogs * 365) if req.cogs else 0
    ccc = dso + dio - dpo
    
    financing_need = max(0, req.current_liabilities - req.current_assets)
    
    if current_ratio >= 2 and quick_ratio >= 1:
        rec = "Strong working capital position - no immediate financing needed"
    elif current_ratio >= 1:
        rec = "Adequate but monitor - consider optimizing inventory and receivables"
    else:
        rec = "Working capital deficit - immediate short-term financing recommended"
    
    return WorkingCapitalResult(
        company_id=req.company_id, period=req.period,
        working_capital=round(wc, 2), current_ratio=round(current_ratio, 2),
        quick_ratio=round(quick_ratio, 2),
        cash_conversion_cycle=round(ccc, 1),
        dso=round(dso, 1), dio=round(dio, 1), dpo=round(dpo, 1),
        financing_need=round(financing_need, 2), recommendation=rec
    )

@app.post("/factoring", response_model=FactoringResult)
async def calculate_factoring(req: FactoringRequest):
    advance = req.invoice_amount * req.advance_rate
    fee = req.invoice_amount * req.factor_fee_rate
    discount = req.invoice_amount * req.discount_rate
    net = advance - fee
    total_cost = fee + discount
    ear = (total_cost / (advance - total_cost)) * 365 / 60 * 100 if advance > total_cost else 0
    
    return FactoringResult(
        invoice_amount=round(req.invoice_amount, 2),
        advance_amount=round(advance, 2),
        fee=round(fee, 2), discount=round(discount, 2),
        net_to_company=round(net, 2), total_cost=round(total_cost, 2),
        effective_annual_rate=round(ear, 2)
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
