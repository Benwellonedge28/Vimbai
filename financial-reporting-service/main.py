"""
Financial Reporting Service
Port: 8267
Financial statement preparation
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Financial Reporting Service", version="1.0.0")


class FinancialReportingRequest(BaseModel):
    company_id: str
    period: str
    balance_sheet: Dict[str, float]
    income_statement: Dict[str, float]
    cash_flow: Dict[str, float]


class FinancialReportingResponse(BaseModel):
    company_id: str
    report_date: str
    period: str
    financial_statements: Dict[str, Any]
    key_metrics: Dict[str, float]
    report_status: str
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "financial-reporting", "version": "1.0.0"}


@app.post("/generate", response_model=FinancialReportingResponse)
async def generate_financial_report(request: FinancialReportingRequest):
    logger.info("Generating financial report", company=request.company_id, period=request.period)

    total_assets = sum(request.balance_sheet.get(k, 0) for k in ["current_assets", "non_current_assets"])
    total_liabilities = sum(request.balance_sheet.get(k, 0) for k in ["current_liabilities", "non_current_liabilities"])
    equity = total_assets - total_liabilities

    revenue = request.income_statement.get("revenue", 0)
    net_income = request.income_statement.get("net_income", 0)
    ebit = request.income_statement.get("ebit", 0)

    financial_statements = {
        "balance_sheet": {
            **request.balance_sheet,
            "total_assets": round(total_assets, 2),
            "total_liabilities": round(total_liabilities, 2),
            "total_equity": round(equity, 2),
        },
        "income_statement": {
            **request.income_statement,
            "gross_margin": (
                round((revenue - request.income_statement.get("cogs", 0)) / revenue * 100, 2) if revenue else 0
            ),
            "net_margin": round(net_income / revenue * 100, 2) if revenue else 0,
        },
        "cash_flow": request.cash_flow,
    }

    key_metrics = {
        "roe": round(net_income / equity * 100, 2) if equity else 0,
        "roa": round(net_income / total_assets * 100, 2) if total_assets else 0,
        "current_ratio": round(
            request.balance_sheet.get("current_assets", 0) / request.balance_sheet.get("current_liabilities", 1), 2
        ),
        "debt_to_equity": round(total_liabilities / equity, 2) if equity else 0,
    }

    recommendations = []
    if key_metrics["roe"] < 10:
        recommendations.append("Low ROE - improve profitability or efficiency")
    if key_metrics["current_ratio"] < 1.5:
        recommendations.append("Low liquidity - improve working capital")

    return FinancialReportingResponse(
        company_id=request.company_id,
        report_date=datetime.now().isoformat(),
        period=request.period,
        financial_statements=financial_statements,
        key_metrics=key_metrics,
        report_status="Ready",
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8267)
