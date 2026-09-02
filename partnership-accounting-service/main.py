"""
Vimbai Partnership Accounting Service
Partner capital accounts, profit sharing, and partnership financial statements.
Port: 8339
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "partnership-accounting-service"
PORT = int(os.getenv("PORT", "8339"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Partnership Accounting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class Partner(BaseModel):
    partner_id: str; name: str; capital_contribution: float
    profit_share_pct: float; salary: float = 0; interest_on_capital_rate: float = 0.05
    drawings: float = 0

class PartnershipRequest(BaseModel):
    company_id: str; period: str; partners: List[Partner]
    net_profit: float; interest_rate_on_capital: float = 0.05

class PartnerAccount(BaseModel):
    partner_id: str; name: str
    opening_capital: float; salary: float; interest_on_capital: float
    profit_share: float; drawings: float; closing_capital: float

class PartnershipResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; period: str
    total_salary: float; total_interest: float; total_profit_share: float
    residual_profit: float; partner_accounts: List[PartnerAccount]

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/allocate", response_model=PartnershipResult)
async def allocate_profit(req: PartnershipRequest):
    total_salary = sum(p.salary for p in req.partners)
    total_interest = sum(p.capital_contribution * req.interest_rate_on_capital for p in req.partners)
    
    profit_after_salary_interest = req.net_profit - total_salary - total_interest
    profit_after_salary_interest = max(profit_after_salary_interest, 0)
    
    total_share_pct = sum(p.profit_share_pct for p in req.partners)
    if total_share_pct == 0:
        total_share_pct = 100
        equal_share = 100 / len(req.partners)
        for p in req.partners:
            p.profit_share_pct = equal_share
    
    total_profit_share = 0
    accounts = []
    for p in req.partners:
        interest = p.capital_contribution * req.interest_rate_on_capital
        profit_share = profit_after_salary_interest * (p.profit_share_pct / 100)
        total_profit_share += profit_share
        closing_capital = p.capital_contribution + p.salary + interest + profit_share - p.drawings
        
        accounts.append(PartnerAccount(
            partner_id=p.partner_id, name=p.name,
            opening_capital=p.capital_contribution, salary=p.salary,
            interest_on_capital=round(interest, 2),
            profit_share=round(profit_share, 2), drawings=p.drawings,
            closing_capital=round(closing_capital, 2)
        ))
    
    return PartnershipResult(
        company_id=req.company_id, period=req.period,
        total_salary=round(total_salary, 2), total_interest=round(total_interest, 2),
        total_profit_share=round(total_profit_share, 2),
        residual_profit=round(profit_after_salary_interest, 2),
        partner_accounts=accounts
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
