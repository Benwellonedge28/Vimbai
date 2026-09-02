"""
Vimbai VAT Reporting Service
VAT calculation, returns, and compliance reporting.
Port: 8351
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "vat-reporting-service"
PORT = int(os.getenv("PORT", "8351"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai VAT Reporting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

VAT_STANDARD_RATE = 0.15
VAT_ZERO_RATE = 0.0
VAT_EXEMPT = "exempt"


class VATTransaction(BaseModel):
    description: str
    amount: float
    vat_rate: float = VAT_STANDARD_RATE
    transaction_type: str = "standard"  # standard, zero_rated, exempt


class VATReturnRequest(BaseModel):
    company_id: str
    tax_period: str  # e.g. "2026-Q1"
    input_transactions: List[VATTransaction] = []  # purchases
    output_transactions: List[VATTransaction] = []  # sales


class VATReturn(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    tax_period: str
    total_output_vat: float
    total_input_vat: float
    net_vat_payable: float
    net_vat_refundable: float
    standard_rated_sales: float
    zero_rated_sales: float
    exempt_sales: float
    standard_rated_purchases: float
    vat_due_to_authority: float
    submission_date: str = Field(default_factory=lambda: datetime.now(timezone.utc).isoformat())


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/prepare-return", response_model=VATReturn)
async def prepare_vat_return(req: VATReturnRequest):
    total_output_vat = 0
    total_input_vat = 0
    std_sales = 0
    zero_sales = 0
    exempt_sales = 0
    std_purchases = 0

    for tx in req.output_transactions:
        vat = tx.amount * tx.vat_rate
        total_output_vat += vat
        if tx.transaction_type == "standard":
            std_sales += tx.amount
        elif tx.transaction_type == "zero_rated":
            zero_sales += tx.amount
        else:
            exempt_sales += tx.amount

    for tx in req.input_transactions:
        vat = tx.amount * tx.vat_rate
        total_input_vat += vat
        if tx.transaction_type == "standard":
            std_purchases += tx.amount

    net = total_output_vat - total_input_vat
    payable = max(net, 0)
    refundable = max(-net, 0)

    return VATReturn(
        company_id=req.company_id,
        tax_period=req.tax_period,
        total_output_vat=round(total_output_vat, 2),
        total_input_vat=round(total_input_vat, 2),
        net_vat_payable=round(payable, 2),
        net_vat_refundable=round(refundable, 2),
        standard_rated_sales=round(std_sales, 2),
        zero_rated_sales=round(zero_sales, 2),
        exempt_sales=round(exempt_sales, 2),
        standard_rated_purchases=round(std_purchases, 2),
        vat_due_to_authority=round(payable, 2),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
