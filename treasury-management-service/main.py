"""
Vimbai Treasury Management Service
Cash flow forecasting, liquidity management, and treasury operations.
Port: 8320
"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "treasury-management-service"
PORT = int(os.getenv("PORT", "8320"))

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

app = FastAPI(title="Vimbai Treasury Management Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="treasury-management-service", instrument_app=app)
except ImportError:
    TRACER = None


class CashFlowType(str, Enum):
    INFLOW = "inflow"
    OUTFLOW = "outflow"
    INVESTMENT = "investment"
    FINANCING = "financing"


class LiquidityLevel(str, Enum):
    EXCESS = "excess"
    ADEQUATE = "adequate"
    TIGHT = "tight"
    CRITICAL = "critical"


class CashFlowEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    account_id: str = ""
    flow_type: CashFlowType
    amount: float
    currency: str = "USD"
    date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    description: str = ""
    category: str = ""


class CashPosition(BaseModel):
    company_id: str
    total_cash: float
    currency: str = "USD"
    available_cash: float
    restricted_cash: float = 0
    short_term_investments: float = 0
    liquidity_level: LiquidityLevel = LiquidityLevel.ADEQUATE
    as_of: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashFlowForecast(BaseModel):
    company_id: str
    period_start: datetime
    period_end: datetime
    projected_inflows: float
    projected_outflows: float
    net_cash_flow: float
    ending_position: float
    confidence: float = 0.75
    assumptions: List[str] = []


class InvestmentOption(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    instrument_type: str  # money_market, treasury_bill, bond, fixed_deposit
    expected_return: float
    duration_days: int
    min_amount: float
    risk_level: str = "low"
    liquidity: str = "high"


_cashflows: Dict[str, List[CashFlowEntry]] = defaultdict(list)
_positions: Dict[str, CashPosition] = {}


def assess_liquidity(cash: float, monthly_burn: float) -> LiquidityLevel:
    if monthly_burn <= 0:
        return LiquidityLevel.EXCESS
    months_runway = cash / monthly_burn
    if months_runway > 6:
        return LiquidityLevel.EXCESS
    if months_runway > 3:
        return LiquidityLevel.ADEQUATE
    if months_runway > 1:
        return LiquidityLevel.TIGHT
    return LiquidityLevel.CRITICAL


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/cashflows")
async def record_cashflow(entry: CashFlowEntry):
    _cashflows[entry.company_id].append(entry)
    logger.info("cashflow_recorded", company_id=entry.company_id, type=entry.flow_type, amount=entry.amount)
    return {"id": entry.id, "status": "recorded"}


@app.get("/cashflows/{company_id}")
async def get_cashflows(company_id: str, limit: int = 100):
    flows = _cashflows.get(company_id, [])
    return {"company_id": company_id, "cashflows": flows[-limit:], "total": len(flows)}


@app.get("/position/{company_id}")
async def get_cash_position(company_id: str):
    if company_id in _positions:
        return _positions[company_id]
    flows = _cashflows.get(company_id, [])
    total = sum(f.amount if f.flow_type == CashFlowType.INFLOW else -f.amount for f in flows)
    pos = CashPosition(company_id=company_id, total_cash=total, available_cash=total)
    _positions[company_id] = pos
    return pos


@app.put("/position/{company_id}")
async def update_cash_position(company_id: str, position: CashPosition):
    position.company_id = company_id
    monthly_burn = abs(
        sum(
            f.amount
            for f in _cashflows.get(company_id, [])
            if f.flow_type == CashFlowType.OUTFLOW and f.date > datetime.now(timezone.utc) - timedelta(days=30)
        )
    )
    position.liquidity_level = assess_liquidity(position.available_cash, monthly_burn)
    _positions[company_id] = position
    return position


@app.post("/forecast/{company_id}")
async def generate_forecast(company_id: str, days: int = 30):
    flows = _cashflows.get(company_id, [])
    if not flows:
        return CashFlowForecast(
            company_id=company_id,
            period_start=datetime.now(timezone.utc),
            period_end=datetime.now(timezone.utc) + timedelta(days=days),
            projected_inflows=0,
            projected_outflows=0,
            net_cash_flow=0,
            ending_position=0,
            assumptions=["No historical data - forecast is zero-based"],
        )
    avg_daily_inflow = sum(f.amount for f in flows if f.flow_type == CashFlowType.INFLOW) / max(
        1, (datetime.now(timezone.utc) - flows[0].date).days
    )
    avg_daily_outflow = abs(sum(f.amount for f in flows if f.flow_type == CashFlowType.OUTFLOW)) / max(
        1, (datetime.now(timezone.utc) - flows[0].date).days
    )
    projected_in = avg_daily_inflow * days
    projected_out = avg_daily_outflow * days
    current = _positions.get(
        company_id, CashPosition(company_id=company_id, total_cash=0, available_cash=0)
    ).available_cash
    return CashFlowForecast(
        company_id=company_id,
        period_start=datetime.now(timezone.utc),
        period_end=datetime.now(timezone.utc) + timedelta(days=days),
        projected_inflows=round(projected_in, 2),
        projected_outflows=round(projected_out, 2),
        net_cash_flow=round(projected_in - projected_out, 2),
        ending_position=round(current + projected_in - projected_out, 2),
        assumptions=[
            f"Average daily inflow: {avg_daily_inflow:.2f}",
            f"Average daily outflow: {avg_daily_outflow:.2f}",
            f"Forecast period: {days} days",
        ],
    )


@app.get("/investment-options")
async def get_investment_options():
    options = [
        InvestmentOption(
            name="Money Market Fund",
            instrument_type="money_market",
            expected_return=0.045,
            duration_days=30,
            min_amount=1000,
            risk_level="low",
            liquidity="high",
        ),
        InvestmentOption(
            name="Treasury Bill 91-Day",
            instrument_type="treasury_bill",
            expected_return=0.07,
            duration_days=91,
            min_amount=5000,
            risk_level="very_low",
            liquidity="medium",
        ),
        InvestmentOption(
            name="Fixed Deposit 6M",
            instrument_type="fixed_deposit",
            expected_return=0.06,
            duration_days=180,
            min_amount=10000,
            risk_level="very_low",
            liquidity="low",
        ),
        InvestmentOption(
            name="Government Bond 2Y",
            instrument_type="bond",
            expected_return=0.085,
            duration_days=730,
            min_amount=25000,
            risk_level="low",
            liquidity="low",
        ),
    ]
    return {"options": options}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
