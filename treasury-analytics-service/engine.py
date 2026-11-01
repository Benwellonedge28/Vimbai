"""Treasury KPI calculation.

The figures come from the Rust library when its extension is installed; the
Python maths stays here as the reference implementation and as the fallback
for images built without the wheel. They are held to the same answers by
tests/test_native_seam.py, which runs both against the live service.

VIMBAI_CALC_ENGINE selects the engine: "auto" (default) uses Rust when it is
available, "rust" refuses to start without it, "python" pins the reference.
"""

import json
import os
from decimal import ROUND_HALF_EVEN, Decimal, getcontext, localcontext

from treasury_analytics_service import models

# Vimbai holds any figure at any magnitude: the default 28-digit decimal
# context cannot even represent a 30-digit balance.
getcontext().prec = 200
getcontext().rounding = ROUND_HALF_EVEN

ENGINE_ENV = "VIMBAI_CALC_ENGINE"
AMOUNT_FIELDS = (
    "total_cash",
    "monthly_inflow",
    "monthly_outflow",
    "short_term_debt",
    "total_debt",
    "investments",
    "fx_exposure",
)


class EngineUnavailable(RuntimeError):
    """Raised when the requested engine cannot be used."""


def _extension():
    try:
        import vimbai_native
    except ImportError:
        return None
    return vimbai_native


def engine_in_use() -> str:
    """Announce the calculator so /health can be read in production."""
    requested = os.getenv(ENGINE_ENV, "auto").strip().lower()
    if requested in ("python", "reference"):
        return "python"
    if _extension() is None:
        if requested == "rust":
            raise EngineUnavailable("VIMBAI_CALC_ENGINE=rust but the vimbai_native extension is not installed")
        return "python"
    return "rust"


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


def _compute_rust(req: models.AnalyticsRequest) -> models.AnalyticsResponse:
    extension = _extension()
    payload = {"company_id": req.company_id}
    payload.update({field: format(getattr(req, field), "f") for field in AMOUNT_FIELDS})
    result = json.loads(extension.analyze_treasury_json(json.dumps(payload)))
    return models.AnalyticsResponse(
        company_id=result["company_id"],
        kpis=[
            models.TreasuryKPI(
                name=kpi["name"],
                value=Decimal(kpi["value"]),
                unit=kpi["unit"],
                benchmark=Decimal(kpi["benchmark"]),
                status=kpi["status"],
                description=kpi["description"],
            )
            for kpi in result["kpis"]
        ],
        cash_adequacy_days=Decimal(result["cash_adequacy_days"]),
        debt_service_ratio=Decimal(result["debt_service_ratio"]),
        investment_yield=Decimal(result["investment_yield"]),
        fx_risk_score=Decimal(result["fx_risk_score"]),
    )


def compute(req: models.AnalyticsRequest) -> models.AnalyticsResponse:
    """Compute the KPI set with the engine this deployment is running."""
    if engine_in_use() == "rust":
        return _compute_rust(req)
    return _compute_python(req)


def _compute_python(req: models.AnalyticsRequest) -> models.AnalyticsResponse:
    """The KPI set as Python computed it before the Rust port."""
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
    return models.AnalyticsResponse(
        company_id=req.company_id,
        kpis=kpis,
        cash_adequacy_days=cash_adequacy,
        debt_service_ratio=debt_service,
        investment_yield=yield_pct,
        fx_risk_score=fx_score,
    )
