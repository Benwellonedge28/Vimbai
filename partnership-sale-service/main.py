"""
Vimbai Partnership Sale Service
Handles partner retirement, admission, and partnership dissolution calculations.
Port: 8340
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "partnership-sale-service"
PORT = int(os.getenv("PORT", "8340"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Partnership Sale Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class PartnerBuyoutRequest(BaseModel):
    company_id: str; outgoing_partner_id: str; outgoing_partner_name: str
    capital_balance: float; share_of_goodwill: float = 0
    share_of_reserves: float = 0; loan_to_partner: float = 0
    agreed_payment: float; remaining_partners: List[Dict] = []

class BuyoutResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; outgoing_partner: str
    capital_due: float; goodwill_due: float; reserves_due: float
    loan_due: float; total_payable: float
    agreed_payment: float; difference: float
    gain_or_loss_on_retirement: float
    remaining_partner_adjustments: List[Dict] = []

class GoodwillRequest(BaseModel):
    company_id: str; partners: List[Dict]; avg_profit: float; years: int = 3; rate: float = 0.15

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/retirement", response_model=BuyoutResult)
async def calculate_retirement(req: PartnerBuyoutRequest):
    total_payable = req.capital_balance + req.share_of_goodwill + req.share_of_reserves + req.loan_to_partner
    difference = total_payable - req.agreed_payment
    gain_or_loss = req.agreed_payment - total_payable
    
    adjustments = []
    if req.remaining_partners:
        total_share = sum(float(p.get("share_pct", 0)) for p in req.remaining_partners)
        for p in req.remaining_partners:
            share = float(p.get("share_pct", 0)) / total_share if total_share else 0
            adjustment = difference * share
            adjustments.append({
                "partner_id": p.get("partner_id", ""),
                "name": p.get("name", ""),
                "capital_adjustment": round(adjustment, 2),
                "share_pct": p.get("share_pct", 0)
            })
    
    return BuyoutResult(
        company_id=req.company_id, outgoing_partner=req.outgoing_partner_name,
        capital_due=round(req.capital_balance, 2), goodwill_due=round(req.share_of_goodwill, 2),
        reserves_due=round(req.share_of_reserves, 2), loan_due=round(req.loan_to_partner, 2),
        total_payable=round(total_payable, 2), agreed_payment=round(req.agreed_payment, 2),
        difference=round(difference, 2), gain_or_loss_on_retirement=round(gain_or_loss, 2),
        remaining_partner_adjustments=adjustments
    )

@app.post("/goodwill", response_model=dict)
async def calculate_goodwill(req: GoodwillRequest):
    total_capital = sum(float(p.get("capital", 0)) for p in req.partners)
    super_profit = req.avg_profit - (total_capital * req.rate)
    goodwill = super_profit * req.years
    
    partner_goodwill = {}
    for p in req.partners:
        share = float(p.get("share_pct", 0)) / 100
        partner_goodwill[p.get("partner_id", "")] = round(goodwill * share, 2)
    
    return {
        "company_id": req.company_id,
        "total_goodwill": round(goodwill, 2),
        "super_profit": round(super_profit, 2),
        "normal_return_rate": req.rate,
        "partner_goodwill_shares": partner_goodwill
    }

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
