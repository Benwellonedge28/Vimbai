"""
IFRS Reporting Service
Port: 8268
IFRS-compliant financial reporting
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="IFRS Reporting Service", version="1.0.0")


class IFRSReportingRequest(BaseModel):
    company_id: str
    ifrs_standards: List[str]
    financial_data: Dict[str, Any]
    adjustments: Dict[str, Any]


class IFRSReportingResponse(BaseModel):
    company_id: str
    report_date: str
    ifrs_statements: Dict[str, Any]
    ifrs_metrics: Dict[str, float]
    compliance_check: Dict[str, Any]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "ifrs-reporting", "version": "1.0.0"}


@app.post("/generate", response_model=IFRSReportingResponse)
async def generate_ifrs_report(request: IFRSReportingRequest):
    logger.info("Generating IFRS report", company=request.company_id)

    ifrs_statements = {
        "statement_of_financial_position": {
            "assets": request.financial_data.get("ifrs_assets", {}),
            "liabilities": request.financial_data.get("ifrs_liabilities", {}),
            "equity": request.financial_data.get("ifrs_equity", {}),
        },
        "statement_of_profit_or_loss": {
            "revenue": request.financial_data.get("ifrs_revenue", 0),
            "operating_profit": request.financial_data.get("ifrs_ebit", 0),
            "profit_after_tax": request.financial_data.get("ifrs_net_income", 0),
        },
        "ifrs_16_adjustments": request.adjustments.get("ifrs_16", {}),
        "ifrs_9_adjustments": request.adjustments.get("ifrs_9", {}),
        "ifrs_15_adjustments": request.adjustments.get("ifrs_15", {}),
    }

    ifrs_metrics = {
        "basic_eps": round(
            request.financial_data.get("ifrs_net_income", 0) / request.financial_data.get("shares", 1), 4
        ),
        "diluted_eps": round(
            request.financial_data.get("ifrs_net_income", 0) / request.financial_data.get("diluted_shares", 1), 4
        ),
        "tangible_assets": round(
            request.financial_data.get("ifrs_assets", {}).get("total", 0)
            - request.financial_data.get("ifrs_intangibles", 0),
            2,
        ),
    }

    compliance_check = {"standards_applied": request.ifrs_standards, "compliance_rate": 95.0, "issues": []}

    recommendations = []
    if "IFRS 16" not in request.ifrs_standards:
        recommendations.append("Ensure IFRS 16 lease accounting is applied")
    if ifrs_metrics["diluted_eps"] < ifrs_metrics["basic_eps"] * 0.95:
        recommendations.append("Dilution effect significant - disclose potential impact")

    return IFRSReportingResponse(
        company_id=request.company_id,
        report_date=datetime.now().isoformat(),
        ifrs_statements=ifrs_statements,
        ifrs_metrics=ifrs_metrics,
        compliance_check=compliance_check,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8268)
