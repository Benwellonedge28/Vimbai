"""Vimbai Treasury Analytics Service - Analytics and KPIs for treasury operations. Port: 8322"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from typing import Any, Dict, List

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "treasury-analytics-service"
PORT = int(os.getenv("PORT", "8322"))
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
app = FastAPI(title="Vimbai Treasury Analytics Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="treasury-analytics-service", instrument_app=app)
except ImportError:
    TRACER = None


class TreasuryKPI(BaseModel):
    name: str
    value: float
    unit: str
    benchmark: float = 0
    status: str = "good"
    description: str = ""


class AnalyticsRequest(BaseModel):
    company_id: str
    total_cash: float = 0
    monthly_inflow: float = 0
    monthly_outflow: float = 0
    short_term_debt: float = 0
    total_debt: float = 0
    investments: float = 0
    fx_exposure: float = 0


class AnalyticsResponse(BaseModel):
    company_id: str
    kpis: List[TreasuryKPI]
    cash_adequacy_days: float
    debt_service_ratio: float
    investment_yield: float
    fx_risk_score: float


_metrics: Dict[str, AnalyticsResponse] = {}


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/analyze", response_model=AnalyticsResponse)
async def analyze_treasury(req: AnalyticsRequest):
    net_flow = req.monthly_inflow - req.monthly_outflow
    cash_adequacy = req.total_cash / max(1, req.monthly_outflow) * 30 if req.monthly_outflow > 0 else 999
    debt_service = (req.short_term_debt / max(1, req.monthly_inflow)) * 100 if req.monthly_inflow > 0 else 0
    yield_pct = (req.investments * 0.05) / max(1, req.total_cash) * 100 if req.total_cash > 0 else 0
    fx_score = min(100, (req.fx_exposure / max(1, req.total_cash)) * 100)

    kpis = [
        TreasuryKPI(
            name="Net Cash Flow",
            value=net_flow,
            unit="USD",
            status="good" if net_flow > 0 else "warning",
            description="Monthly net cash position",
        ),
        TreasuryKPI(
            name="Cash Runway",
            value=cash_adequacy,
            unit="days",
            benchmark=90,
            status="good" if cash_adequacy > 90 else "warning" if cash_adequacy > 30 else "critical",
            description="Days of cash available at current burn",
        ),
        TreasuryKPI(
            name="Debt Service Ratio",
            value=debt_service,
            unit="%",
            benchmark=30,
            status="good" if debt_service < 30 else "warning" if debt_service < 50 else "critical",
            description="Short-term debt as % of monthly inflow",
        ),
        TreasuryKPI(
            name="Investment Yield",
            value=yield_pct,
            unit="%",
            benchmark=5,
            status="good" if yield_pct >= 5 else "warning",
            description="Estimated annual yield on investments",
        ),
        TreasuryKPI(
            name="FX Risk Score",
            value=fx_score,
            unit="score",
            benchmark=20,
            status="good" if fx_score < 20 else "warning" if fx_score < 50 else "critical",
            description="Foreign exchange exposure risk",
        ),
        TreasuryKPI(
            name="Cash Utilization",
            value=(
                (1 - req.total_cash / max(1, req.total_cash + req.investments)) * 100
                if (req.total_cash + req.investments) > 0
                else 0
            ),
            unit="%",
            benchmark=70,
            status="good",
            description="Cash deployed in investments vs idle",
        ),
    ]
    resp = AnalyticsResponse(
        company_id=req.company_id,
        kpis=kpis,
        cash_adequacy_days=cash_adequacy,
        debt_service_ratio=debt_service,
        investment_yield=yield_pct,
        fx_risk_score=fx_score,
    )
    _metrics[req.company_id] = resp
    return resp


@app.get("/kpi/{company_id}")
async def get_kpis(company_id: str):
    if company_id in _metrics:
        return _metrics[company_id]
    return {"company_id": company_id, "message": "Run /analyze first"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
