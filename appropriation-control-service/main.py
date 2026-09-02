"""Vimbai Appropriation Control Service - Budget appropriation and spending control. Port: 8367"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "appropriation-control-service"
PORT = int(os.getenv("PORT", "8367"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Appropriation Control Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="appropriation-control-service", instrument_app=app)
except ImportError:
    TRACER = None


class Appropriation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    department: str
    fiscal_year: str
    approved_amount: float
    spent_amount: float = 0
    committed_amount: float = 0
    available_amount: float = 0
    status: str = "active"  # active, exhausted, closed


class AppropriationTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    appropriation_id: str
    type: str = "commit"  # commit, spend, uncommit, refund
    amount: float
    description: str = ""
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


_appropriations: Dict[str, List[Appropriation]] = defaultdict(list)
_transactions: Dict[str, List[AppropriationTransaction]] = defaultdict(list)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/appropriations", response_model=Appropriation)
async def create_appropriation(appr: Appropriation):
    appr.available_amount = appr.approved_amount - appr.committed_amount - appr.spent_amount
    _appropriations[appr.company_id].append(appr)
    return appr


@app.get("/appropriations/{company_id}")
async def get_appropriations(company_id: str, department: Optional[str] = None):
    apprs = _appropriations.get(company_id, [])
    if department:
        apprs = [a for a in apprs if a.department == department]
    return {"company_id": company_id, "appropriations": apprs, "total": len(apprs)}


@app.post("/transactions")
async def create_transaction(tx: AppropriationTransaction):
    _transactions[tx.appropriation_id].append(tx)
    for apprs in _appropriations.values():
        for a in apprs:
            if a.id == tx.appropriation_id:
                if tx.type == "commit":
                    a.committed_amount += tx.amount
                elif tx.type == "spend":
                    a.spent_amount += tx.amount
                    a.committed_amount -= tx.amount
                elif tx.type == "uncommit":
                    a.committed_amount -= tx.amount
                elif tx.type == "refund":
                    a.spent_amount -= tx.amount
                a.available_amount = a.approved_amount - a.committed_amount - a.spent_amount
                if a.available_amount <= 0:
                    a.status = "exhausted"
                return {"id": tx.id, "available": a.available_amount, "status": a.status}
    raise HTTPException(status_code=404, detail="Appropriation not found")


@app.get("/check/{appropriation_id}")
async def check_available(appropriation_id: str, amount: float):
    for apprs in _appropriations.values():
        for a in apprs:
            if a.id == appropriation_id:
                can_spend = a.available_amount >= amount
                return {
                    "appropriation_id": appropriation_id,
                    "available": a.available_amount,
                    "requested": amount,
                    "allowed": can_spend,
                }
    raise HTTPException(status_code=404, detail="Appropriation not found")


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
