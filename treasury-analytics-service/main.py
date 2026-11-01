"""Vimbai Treasury Analytics Service - Analytics and KPIs for treasury operations. Port: 8322

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "treasury_analytics_service" not in _sys.modules or not hasattr(
    _sys.modules.get("treasury_analytics_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("treasury_analytics_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["treasury_analytics_service"] = _pkg
    _sys.modules["treasury_analytics_service"].__path__ = [_HERE]

import os
from decimal import ROUND_HALF_EVEN, Decimal, getcontext, localcontext

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from treasury_analytics_service import crud, models
from treasury_analytics_service.dependencies import book_id_var, get_db_session, get_user_id

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

# Vimbai holds any figure at any magnitude: the default 28-digit decimal
# context cannot even represent a 30-digit balance.
getcontext().prec = 200
getcontext().rounding = ROUND_HALF_EVEN


def _working_precision(*values: Decimal) -> int:
    needed = 60
    for value in values:
        if isinstance(value, Decimal) and value.is_finite():
            needed = max(needed, value.adjusted() + len(value.as_tuple().digits) + 40)
    return needed


def round_ratio(value: Decimal, places: int = 10) -> Decimal:
    """Trim a ratio to ten places: enough for any reading, and it removes the
    repeating-tail noise a division leaves behind."""
    with localcontext() as ctx:
        ctx.prec = _working_precision(value) + places
        ctx.rounding = ROUND_HALF_EVEN
        return value.quantize(Decimal(1).scaleb(-places), rounding=ROUND_HALF_EVEN)


def divide_money(numerator: Decimal, denominator: Decimal) -> Decimal:
    """Divide two amounts at a precision wide enough for both."""
    with localcontext() as ctx:
        ctx.prec = _working_precision(numerator, denominator)
        ctx.rounding = ROUND_HALF_EVEN
        return numerator / denominator


app = FastAPI(title="Vimbai Treasury Analytics Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="treasury-analytics-service", instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/analyze", response_model=models.AnalyticsResponse)
async def analyze_treasury(
    req: models.AnalyticsRequest,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Compute treasury KPIs (pure calculation) and persist the snapshot for the caller's Book."""
    net_flow = req.monthly_inflow - req.monthly_outflow
    one = Decimal(1)
    hundred = Decimal(100)
    cash_adequacy = (
        round_ratio(divide_money(req.total_cash, max(one, req.monthly_outflow)) * 30)
        if req.monthly_outflow > 0
        else Decimal(999)
    )
    debt_service = (
        round_ratio(divide_money(req.short_term_debt, max(one, req.monthly_inflow)) * hundred)
        if req.monthly_inflow > 0
        else Decimal(0)
    )
    # a flat 5% of investments, whatever the instruments actually earn
    yield_pct = (
        round_ratio(divide_money(req.investments * Decimal("0.05"), max(one, req.total_cash)) * hundred)
        if req.total_cash > 0
        else Decimal(0)
    )
    # capped at 100 but never floored: a net receivable scores negative
    fx_score = min(hundred, round_ratio(divide_money(req.fx_exposure, max(one, req.total_cash)) * hundred))

    kpis = [
        models.TreasuryKPI(
            name="Net Cash Flow",
            value=net_flow,
            unit="USD",
            status="good" if net_flow > 0 else "warning",
            description="Monthly net cash position",
        ),
        models.TreasuryKPI(
            name="Cash Runway",
            value=cash_adequacy,
            unit="days",
            benchmark=Decimal(90),
            status="good" if cash_adequacy > 90 else "warning" if cash_adequacy > 30 else "critical",
            description="Days of cash available at current burn",
        ),
        models.TreasuryKPI(
            name="Debt Service Ratio",
            value=debt_service,
            unit="%",
            benchmark=Decimal(30),
            status="good" if debt_service < 30 else "warning" if debt_service < 50 else "critical",
            description="Short-term debt as % of monthly inflow",
        ),
        models.TreasuryKPI(
            name="Investment Yield",
            value=yield_pct,
            unit="%",
            benchmark=Decimal(5),
            status="good" if yield_pct >= 5 else "warning",
            description="Estimated annual yield on investments",
        ),
        models.TreasuryKPI(
            name="FX Risk Score",
            value=fx_score,
            unit="score",
            benchmark=Decimal(20),
            status="good" if fx_score < 20 else "warning" if fx_score < 50 else "critical",
            description="Foreign exchange exposure risk",
        ),
        models.TreasuryKPI(
            name="Cash Utilization",
            value=(
                round_ratio((one - divide_money(req.total_cash, max(one, req.total_cash + req.investments))) * hundred)
                if (req.total_cash + req.investments) > 0
                else Decimal(0)
            ),
            unit="%",
            benchmark=70,
            status="good",
            description="Cash deployed in investments vs idle",
        ),
    ]
    resp = models.AnalyticsResponse(
        company_id=req.company_id,
        kpis=kpis,
        cash_adequacy_days=cash_adequacy,
        debt_service_ratio=debt_service,
        investment_yield=yield_pct,
        fx_risk_score=fx_score,
    )
    await crud.save_snapshot(db_session, user_id, resp)
    logger.info(
        "treasury_analyzed",
        company_id=req.company_id,
        cash_adequacy_days=cash_adequacy,
        debt_service_ratio=debt_service,
        fx_risk_score=fx_score,
    )
    return resp


@app.get("/kpi/{company_id}")
async def get_kpis(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Return the caller's latest stored KPI snapshot for the company."""
    snapshot = await crud.get_snapshot(db_session, user_id, company_id)
    if snapshot:
        return snapshot
    return {"company_id": company_id, "message": "Run /analyze first"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
