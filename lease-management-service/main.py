"""
Lease Management Service
Port: 8353
Lease accounting and tracking (ASC 842/IFRS 16)
"""

from datetime import date, datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Lease Management Service", version="1.0.0")


class LeaseInput(BaseModel):
    lease_id: str
    lessee_id: str
    asset_type: str
    lease_term_months: int
    payments: List[float]
    payment_frequency: str
    discount_rate: float
    commencement_date: date


class LeaseCalculationRequest(BaseModel):
    company_id: str
    leases: List[LeaseInput]


class LeaseCalculationResponse(BaseModel):
    company_id: str
    total_lease_liability: float
    total_right_of_use_asset: float
    lease_details: List[Dict[str, Any]]
    maturity_schedule: List[Dict[str, Any]]


class LeasePaymentRequest(BaseModel):
    lease_id: str
    payment_amount: float
    payment_date: date


class LeasePaymentResponse(BaseModel):
    lease_id: str
    principal_paid: float
    interest_paid: float
    remaining_balance: float
    liability_reduction: float


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "lease-management", "version": "1.0.0"}


@app.post("/calculate", response_model=LeaseCalculationResponse)
async def calculate_leases(request: LeaseCalculationRequest):
    logger.info("Calculating leases", company=request.company_id, count=len(request.leases))

    total_liability = 0.0
    total_asset = 0.0
    details = []

    for lease in request.leases:
        pv = sum(p / ((1 + lease.discount_rate) ** (i + 1)) for i, p in enumerate(lease.payments))
        liability = pv
        asset = pv
        total_liability += liability
        total_asset += asset
        details.append({"lease_id": lease.lease_id, "liability": round(liability, 2), "asset": round(asset, 2)})

    return LeaseCalculationResponse(
        company_id=request.company_id,
        total_lease_liability=round(total_liability, 2),
        total_right_of_use_asset=round(total_asset, 2),
        lease_details=details,
        maturity_schedule=[{"year": y, "amount": round(total_liability * 0.2, 2)} for y in range(1, 6)],
    )


@app.post("/payment", response_model=LeasePaymentResponse)
async def process_lease_payment(request: LeasePaymentRequest):
    logger.info("Processing lease payment", lease=request.lease_id)

    return LeasePaymentResponse(
        lease_id=request.lease_id,
        principal_paid=round(request.payment_amount * 0.7, 2),
        interest_paid=round(request.payment_amount * 0.3, 2),
        remaining_balance=100000.0,
        liability_reduction=round(request.payment_amount * 0.7, 2),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8353)
