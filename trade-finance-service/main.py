"""
Vimbai Trade Finance Service
Letter of credit, documentary collection, and trade finance instrument management.
Port: 8380
"""

import os
import uuid
from datetime import datetime, timedelta, timezone
from enum import Enum
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "trade-finance-service"
PORT = int(os.getenv("PORT", "8380"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Trade Finance Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class InstrumentType(str, Enum):
    LETTER_OF_CREDIT = "letter_of_credit"
    DOCUMENTARY_COLLECTION = "documentary_collection"
    BANK_GUARANTEE = "bank_guarantee"
    ADVANCE_PAYMENT = "advance_payment"
    FACTORING = "factoring"


class TradeInstrument(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    instrument_type: InstrumentType
    counterparty: str
    amount: float
    currency: str = "USD"
    issue_date: str = Field(default_factory=lambda: datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    expiry_date: str = ""
    status: str = "issued"  # issued, presented, accepted, paid, expired
    issuing_bank: str = ""
    confirming_bank: str = ""


class InstrumentResult(BaseModel):
    id: str
    company_id: str
    instrument_type: str
    amount: float
    fee_estimate: float
    status: str
    risk_assessment: str
    documentation_required: List[str] = []


_instruments: Dict[str, List[TradeInstrument]] = {}


def _estimate_fee(itype: InstrumentType, amount: float) -> float:
    rates = {
        InstrumentType.LETTER_OF_CREDIT: 0.002,
        InstrumentType.DOCUMENTARY_COLLECTION: 0.001,
        InstrumentType.BANK_GUARANTEE: 0.015,
        InstrumentType.ADVANCE_PAYMENT: 0.0,
        InstrumentType.FACTORING: 0.03,
    }
    return amount * rates.get(itype, 0.002)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/instruments", response_model=InstrumentResult)
async def create_instrument(inst: TradeInstrument):
    _instruments.setdefault(inst.company_id, []).append(inst)
    fee = _estimate_fee(inst.instrument_type, inst.amount)

    risk = "low" if inst.amount < 100000 else "medium" if inst.amount < 500000 else "high"
    docs = ["Commercial invoice", "Bill of lading", "Certificate of origin", "Packing list"]
    if inst.instrument_type == InstrumentType.LETTER_OF_CREDIT:
        docs.extend(["LC application", "Proforma invoice"])

    return InstrumentResult(
        id=inst.id,
        company_id=inst.company_id,
        instrument_type=inst.instrument_type.value,
        amount=inst.amount,
        fee_estimate=round(fee, 2),
        status=inst.status,
        risk_assessment=risk,
        documentation_required=docs,
    )


@app.get("/instruments", response_model=List[TradeInstrument])
async def list_instruments(company_id: str, status: str = ""):
    items = _instruments.get(company_id, [])
    if status:
        items = [i for i in items if i.status == status]
    return items


@app.post("/instruments/{instrument_id}/present")
async def present_documents(company_id: str, instrument_id: str):
    items = _instruments.get(company_id, [])
    for i in items:
        if i.id == instrument_id:
            i.status = "presented"
            return {"instrument_id": instrument_id, "status": "presented"}
    from fastapi import HTTPException

    raise HTTPException(status_code=404, detail="Instrument not found")


@app.post("/instruments/{instrument_id}/settle")
async def settle_instrument(company_id: str, instrument_id: str):
    items = _instruments.get(company_id, [])
    for i in items:
        if i.id == instrument_id:
            i.status = "paid"
            return {"instrument_id": instrument_id, "status": "paid", "amount": i.amount}
    from fastapi import HTTPException

    raise HTTPException(status_code=404, detail="Instrument not found")


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
