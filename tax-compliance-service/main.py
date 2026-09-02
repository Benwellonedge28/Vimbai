"""
Vimbai Tax Compliance Service
Tax filing deadlines, compliance status tracking, and obligation management.
Port: 8374
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
from enum import Enum
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "tax-compliance-service"
PORT = int(os.getenv("PORT", "8374"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Tax Compliance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class ComplianceStatus(str, Enum):
    COMPLIANT = "compliant"; PENDING = "pending"; OVERDUE = "overdue"; FILED = "filed"

class TaxObligation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; obligation_type: str  # vat_return, paye, income_tax, withholding, capital_gains
    description: str; due_date: str; amount: float = 0
    status: ComplianceStatus = ComplianceStatus.PENDING
    filing_frequency: str = "monthly"  # monthly, quarterly, annual

class ComplianceSummary(BaseModel):
    company_id: str; total_obligations: int
    compliant: int; pending: int; overdue: int; filed: int
    compliance_score: float; upcoming_deadlines: List[Dict] = []
    obligations: List[TaxObligation] = []

_obligations: Dict[str, List[TaxObligation]] = {}

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/obligations", response_model=TaxObligation)
async def create_obligation(obligation: TaxObligation):
    _obligations.setdefault(obligation.company_id, []).append(obligation)
    return obligation

@app.get("/obligations", response_model=List[TaxObligation])
async def list_obligations(company_id: str, status: str = ""):
    items = _obligations.get(company_id, [])
    if status:
        items = [o for o in items if o.status.value == status]
    return items

@app.post("/obligations/{obligation_id}/file")
async def file_obligation(company_id: str, obligation_id: str, filed_amount: float = 0):
    items = _obligations.get(company_id, [])
    for o in items:
        if o.id == obligation_id:
            o.status = ComplianceStatus.FILED
            o.amount = filed_amount or o.amount
            return {"filed": True, "obligation_id": obligation_id, "amount": o.amount}
    from fastapi import HTTPException; raise HTTPException(status_code=404, detail="Obligation not found")

@app.get("/summary", response_model=ComplianceSummary)
async def get_summary(company_id: str):
    items = _obligations.get(company_id, [])
    now = datetime.now(timezone.utc)
    
    compliant = sum(1 for o in items if o.status == ComplianceStatus.COMPLIANT)
    pending = sum(1 for o in items if o.status == ComplianceStatus.PENDING)
    overdue = sum(1 for o in items if o.status == ComplianceStatus.OVERDUE)
    filed = sum(1 for o in items if o.status == ComplianceStatus.FILED)
    total = len(items)
    score = (compliant + filed) / total * 100 if total else 100
    
    upcoming = []
    for o in items:
        if o.status in (ComplianceStatus.PENDING, ComplianceStatus.OVERDUE):
            try:
                due = datetime.fromisoformat(o.due_date.replace("Z", "+00:00"))
                days_until = (due - now).days
                if days_until <= 30:
                    upcoming.append({
                        "obligation_id": o.id, "type": o.obligation_type,
                        "due_date": o.due_date, "days_until": days_until,
                        "amount": o.amount, "status": o.status.value
                    })
            except Exception:
                pass
    upcoming.sort(key=lambda x: x["days_until"])
    
    return ComplianceSummary(
        company_id=company_id, total_obligations=total,
        compliant=compliant, pending=pending, overdue=overdue, filed=filed,
        compliance_score=round(score, 1), upcoming_deadlines=upcoming[:5],
        obligations=items
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
