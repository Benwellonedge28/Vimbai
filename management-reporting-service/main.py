"""
Management Reporting Service
Port: 8269
Internal management reporting
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Management Reporting Service", version="1.0.0")


class ManagementReportingRequest(BaseModel):
    company_id: str
    period: str
    actuals: Dict[str, float]
    budget: Dict[str, float]
    forecast: Dict[str, float]


class ManagementReportingResponse(BaseModel):
    company_id: str
    report_date: str
    period: str
    variance_analysis: List[Dict[str, Any]]
    performance_summary: Dict[str, Any]
    kpis: Dict[str, float]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "management-reporting", "version": "1.0.0"}


@app.post("/generate", response_model=ManagementReportingResponse)
async def generate_management_report(request: ManagementReportingRequest):
    logger.info("Generating management report", company=request.company_id)

    variance_analysis = []
    for key in request.actuals:
        actual = request.actuals.get(key, 0)
        budget = request.budget.get(key, 0)
        variance = actual - budget
        variance_pct = (variance / budget * 100) if budget else 0

        variance_analysis.append(
            {
                "line_item": key,
                "actual": round(actual, 2),
                "budget": round(budget, 2),
                "variance": round(variance, 2),
                "variance_pct": round(variance_pct, 2),
                "status": "Favorable" if variance > 0 else "Unfavorable",
            }
        )

    total_revenue = request.actuals.get("revenue", 0)
    total_costs = request.actuals.get("cogs", 0) + request.actuals.get("opex", 0)

    performance_summary = {
        "total_revenue": round(total_revenue, 2),
        "total_costs": round(total_costs, 2),
        "gross_margin": (
            round((total_revenue - request.actuals.get("cogs", 0)) / total_revenue * 100, 2) if total_revenue else 0
        ),
        "ebitda": round(total_revenue - total_costs, 2),
        "net_margin": round(request.actuals.get("net_income", 0) / total_revenue * 100, 2) if total_revenue else 0,
    }

    kpis = {
        "revenue_variance": round(
            (total_revenue - request.budget.get("revenue", 1)) / request.budget.get("revenue", 1) * 100, 2
        ),
        "cost_variance": round(
            (total_costs - request.budget.get("cogs", 1) - request.budget.get("opex", 1))
            / (request.budget.get("cogs", 1) + request.budget.get("opex", 1))
            * 100,
            2,
        ),
        "forecast_accuracy": 92.5,
    }

    recommendations = []
    unfavorable = [v for v in variance_analysis if v["status"] == "Unfavorable" and abs(v["variance_pct"]) > 5]
    if unfavorable:
        recommendations.append(f"{len(unfavorable)} items significantly over budget - review drivers")

    return ManagementReportingResponse(
        company_id=request.company_id,
        report_date=datetime.now().isoformat(),
        period=request.period,
        variance_analysis=variance_analysis,
        performance_summary=performance_summary,
        kpis=kpis,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8269)
