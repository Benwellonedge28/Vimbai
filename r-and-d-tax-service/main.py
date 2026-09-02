"""
Vimbai R&D Tax Credit Service
R&D tax credit calculation, qualifying expenditure tracking, and claim preparation.
Port: 8377
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "r-and-d-tax-service"
PORT = int(os.getenv("PORT", "8377"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai R&D Tax Credit Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class RDExpenditure(BaseModel):
    category: str  # wages, supplies, contract_research, overhead
    description: str
    amount: float
    qualifies: bool = True


class RDClaimRequest(BaseModel):
    company_id: str
    fiscal_year: int
    expenditures: List[RDExpenditure]
    credit_rate: float = 0.15  # 15% R&D credit rate
    alternative_rate: float = 0.20  # alternative simplified credit


class RDClaimResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    fiscal_year: int
    total_expenditure: float
    qualifying_expenditure: float
    non_qualifying: float
    credit_regular: float
    credit_alternative: float
    recommended_method: str
    expenditure_breakdown: Dict[str, float] = {}


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/calculate", response_model=RDClaimResult)
async def calculate_credit(req: RDClaimRequest):
    total = sum(e.amount for e in req.expenditures)
    qualifying = sum(e.amount for e in req.expenditures if e.qualifies)
    non_qualifying = total - qualifying

    credit_reg = qualifying * req.credit_rate
    credit_alt = qualifying * req.alternative_rate

    breakdown = {}
    for e in req.expenditures:
        if e.qualifies:
            breakdown[e.category] = breakdown.get(e.category, 0) + e.amount

    recommended = "regular" if credit_reg > credit_alt else "alternative_simplified"

    return RDClaimResult(
        company_id=req.company_id,
        fiscal_year=req.fiscal_year,
        total_expenditure=round(total, 2),
        qualifying_expenditure=round(qualifying, 2),
        non_qualifying=round(non_qualifying, 2),
        credit_regular=round(credit_reg, 2),
        credit_alternative=round(credit_alt, 2),
        recommended_method=recommended,
        expenditure_breakdown={k: round(v, 2) for k, v in breakdown.items()},
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
