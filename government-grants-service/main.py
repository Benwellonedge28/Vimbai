"""
Vimbai Government Grants Service
Accounting for government grants under IAS 20 - recognition, amortization, and disclosure.
Port: 8347
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "government-grants-service"
PORT = int(os.getenv("PORT", "8347"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Government Grants Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class GrantRequest(BaseModel):
    company_id: str; grant_name: str; grant_amount: float
    grant_type: str  # asset, income, forgivable_loan
    related_asset_cost: float = 0; useful_life_years: int = 5
    conditions: List[str] = []; recognition_method: str = "deferred_income"

class GrantResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; grant_name: str; grant_amount: float; grant_type: str
    annual_amortization: float; deferred_income_balance: float
    journal_entries: List[Dict] = []; disclosure_notes: List[str] = []

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/recognize", response_model=GrantResult)
async def recognize_grant(req: GrantRequest):
    annual_amortization = req.grant_amount / req.useful_life_years if req.useful_life_years > 0 else req.grant_amount
    deferred_balance = req.grant_amount - annual_amortization
    
    entries = []
    if req.grant_type == "asset":
        entries.append({"entry": "Initial recognition", "debit": f"Bank/Asset ({req.grant_amount})", "credit": "Deferred Grant Income"})
        entries.append({"entry": "Annual amortization", "debit": "Deferred Grant Income", "credit": f"Grant Income ({annual_amortization})"})
    elif req.grant_type == "income":
        entries.append({"entry": "Grant received", "debit": f"Bank ({req.grant_amount})", "credit": "Grant Income"})
    elif req.grant_type == "forgivable_loan":
        entries.append({"entry": "Initial recognition", "debit": f"Bank ({req.grant_amount})", "credit": "Forgivable Loan"})
        entries.append({"entry": "Forgiveness", "debit": "Forgivable Loan", "credit": "Grant Income"})
    
    disclosures = [
        f"Grant: {req.grant_name}, Amount: ${req.grant_amount:,.2f}",
        f"Accounting policy: {req.recognition_method} method under IAS 20",
        f"Amortization period: {req.useful_life_years} years",
    ]
    if req.conditions:
        disclosures.append(f"Conditions attached: {', '.join(req.conditions)}")
    
    return GrantResult(
        company_id=req.company_id, grant_name=req.grant_name,
        grant_amount=req.grant_amount, grant_type=req.grant_type,
        annual_amortization=round(annual_amortization, 2),
        deferred_income_balance=round(deferred_balance, 2),
        journal_entries=entries, disclosure_notes=disclosures
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
