"""
Vimbai Project Appraisal Service
Investment project evaluation with NPV, IRR, payback period, and profitability index.
Port: 8404
"""

import math
import os
import uuid
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "project-appraisal-service"
PORT = int(os.getenv("PORT", "8404"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Project Appraisal Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class AppraisalRequest(BaseModel):
    company_id: str
    project_name: str
    initial_investment: float
    cash_flows: List[float]  # year 1, year 2, ...
    discount_rate: float = 0.10
    salvage_value: float = 0


class AppraisalResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    project_name: str
    npv: float
    irr: float
    payback_period: float
    discounted_payback: float
    profitability_index: float
    recommendation: str
    cash_flow_analysis: List[Dict] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/appraise", response_model=AppraisalResult)
async def appraise_project(req: AppraisalRequest):
    cf = list(req.cash_flows)
    if req.salvage_value:
        cf[-1] = cf[-1] + req.salvage_value if cf else req.salvage_value

    # NPV
    npv = -req.initial_investment
    for i, c in enumerate(cf):
        npv += c / (1 + req.discount_rate) ** (i + 1)

    # IRR (bisection)
    irr = 0
    for r in [i * 0.01 for i in range(1, 101)]:
        test = -req.initial_investment + sum(c / (1 + r) ** (i + 1) for i, c in enumerate(cf))
        if test <= 0:
            irr = r
            break

    # Payback
    cumulative = -req.initial_investment
    payback = 0
    for i, c in enumerate(cf):
        cumulative += c
        if cumulative >= 0:
            payback = i + (req.initial_investment - sum(cf[:i])) / c if i > 0 else 1
            break

    # Discounted payback
    disc_cum = -req.initial_investment
    disc_payback = 0
    for i, c in enumerate(cf):
        disc_cum += c / (1 + req.discount_rate) ** (i + 1)
        if disc_cum >= 0:
            disc_payback = i + 1
            break

    pi = (npv + req.initial_investment) / req.initial_investment if req.initial_investment else 0

    analysis = []
    cumulative_cf = -req.initial_investment
    for i, c in enumerate(cf):
        disc = c / (1 + req.discount_rate) ** (i + 1)
        cumulative_cf += c
        analysis.append(
            {
                "year": i + 1,
                "cash_flow": round(c, 2),
                "discounted": round(disc, 2),
                "cumulative": round(cumulative_cf, 2),
            }
        )

    if npv > 0 and irr > req.discount_rate:
        rec = f"Accept: NPV positive ({npv:.0f}), IRR ({irr*100:.1f}%) exceeds discount rate"
    else:
        rec = f"Reject: NPV negative or IRR below discount rate"

    return AppraisalResult(
        company_id=req.company_id,
        project_name=req.project_name,
        npv=round(npv, 2),
        irr=round(irr * 100, 1),
        payback_period=round(payback, 1),
        discounted_payback=round(disc_payback, 1),
        profitability_index=round(pi, 3),
        recommendation=rec,
        cash_flow_analysis=analysis,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
