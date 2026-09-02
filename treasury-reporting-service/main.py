"""
Treasury Reporting Service
Port: 8262
Treasury reporting and analytics
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Treasury Reporting Service", version="1.0.0")


class TreasuryReportingRequest(BaseModel):
    company_id: str
    period: str
    cash_balances: Dict[str, float]
    debt_positions: Dict[str, float]
    hedging_positions: Dict[str, float]
    treasury_metrics: Dict[str, float]


class TreasuryReportingResponse(BaseModel):
    company_id: str
    report_date: str
    period: str
    executive_summary: Dict[str, Any]
    detailed_metrics: Dict[str, Any]
    kpis: Dict[str, float]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "treasury-reporting", "version": "1.0.0"}


@app.post("/report", response_model=TreasuryReportingResponse)
async def generate_treasury_report(request: TreasuryReportingRequest):
    logger.info("Generating treasury report", company=request.company_id)

    total_cash = sum(request.cash_balances.values())
    total_debt = sum(request.debt_positions.values())
    net_debt = total_debt - total_cash

    debt_ratio = total_debt / total_cash if total_cash else 0

    executive_summary = {
        "total_cash": round(total_cash, 2),
        "total_debt": round(total_debt, 2),
        "net_debt": round(net_debt, 2),
        "hedge_coverage": round(sum(request.hedging_positions.values()), 2),
    }

    kpis = {
        "cash_to_debt_ratio": round(total_cash / total_debt, 4) if total_debt else 0,
        "debt_to_equity": round(request.treasury_metrics.get("total_equity", 1) / total_debt, 4) if total_debt else 0,
        "interest_coverage": round(
            request.treasury_metrics.get("ebitda", 0) / request.treasury_metrics.get("interest", 1), 4
        ),
        "liquidity_ratio": round(total_cash / request.treasury_metrics.get("current_liabilities", 1), 4),
    }

    recommendations = []
    if kpis["cash_to_debt_ratio"] < 0.3:
        recommendations.append("Low cash-to-debt ratio - improve liquidity")
    if kpis["interest_coverage"] < 3:
        recommendations.append("Low interest coverage - consider debt restructuring")

    return TreasuryReportingResponse(
        company_id=request.company_id,
        report_date=datetime.now().isoformat(),
        period=request.period,
        executive_summary=executive_summary,
        detailed_metrics={"debt_breakdown": request.debt_positions, "hedge_breakdown": request.hedging_positions},
        kpis=kpis,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8262)
