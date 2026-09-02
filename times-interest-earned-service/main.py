"""
Times Interest Earned Service
Port: 8218
Interest coverage ratio analysis
"""

from typing import Any, Dict

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Times Interest Earned Service", version="1.0.0")


class CoverageMetrics(BaseModel):
    times_interest_earned: float
    interest_coverage_ratio: float
    debt_service_coverage_ratio: float
    fixed_charge_coverage: float
    cash_flow_to_debt: float


class TIERequest(BaseModel):
    company_id: str
    period: str
    ebit: float
    interest_expense: float
    principal_repayment: float
    lease_payments: float
    operating_cash_flow: float
    total_debt: float


class TIEResponse(BaseModel):
    company_id: str
    period: str
    coverage_metrics: CoverageMetrics
    industry_benchmark: float
    risk_assessment: str
    covenant_compliance: Dict[str, bool]
    recommendations: list


async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "times-interest-earned", "version": "1.0.0"}


@app.post("/analyze", response_model=TIEResponse)
async def analyze_times_interest_earned(request: TIERequest):
    logger.info("Analyzing times interest earned", company=request.company_id, period=request.period)

    tie = request.ebit / request.interest_expense if request.interest_expense else 0

    debt_service = request.interest_expense + request.principal_repayment
    dsr = request.ebit / debt_service if debt_service else 0

    fixed_charges = request.interest_expense + request.principal_repayment + request.lease_payments
    fcc = request.ebit / fixed_charges if fixed_charges else 0

    cf_to_debt = request.operating_cash_flow / request.total_debt if request.total_debt else 0

    benchmark = 3.0
    risk = "low" if tie >= 4.0 else "medium" if tie >= 2.0 else "high"

    covenant_compliance = {"tie_covenant": tie >= 2.5, "dsr_covenant": dsr >= 1.25, "debt_to_cf": cf_to_debt >= 0.25}

    return TIEResponse(
        company_id=request.company_id,
        period=request.period,
        coverage_metrics=CoverageMetrics(
            times_interest_earned=round(tie, 2),
            interest_coverage_ratio=round(tie, 2),
            debt_service_coverage_ratio=round(dsr, 2),
            fixed_charge_coverage=round(fcc, 2),
            cash_flow_to_debt=round(cf_to_debt, 4),
        ),
        industry_benchmark=benchmark,
        risk_assessment=risk,
        covenant_compliance=covenant_compliance,
        recommendations=[
            "Interest coverage is healthy" if risk == "low" else "Consider debt reduction strategies",
            "Monitor covenant compliance",
            "Review refinancing options for lower interest costs",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8218)
