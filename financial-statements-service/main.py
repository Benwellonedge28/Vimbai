"""
Financial Statements Service
Port: 8332
Financial statement preparation and analysis
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Financial Statements Service", version="1.0.0")


class FinancialStatementsRequest(BaseModel):
    company_id: str
    period: str
    balance_sheet: Dict[str, float]
    income_statement: Dict[str, float]
    cash_flow: Dict[str, float]
    equity_statement: Dict[str, float]


class FinancialStatementsResponse(BaseModel):
    company_id: str
    period: str
    statements: Dict[str, Any]
    key_ratios: Dict[str, float]
    financial_health_score: float
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "financial-statements", "version": "1.0.0"}


@app.post("/prepare", response_model=FinancialStatementsResponse)
async def prepare_financial_statements(request: FinancialStatementsRequest):
    logger.info("Preparing financial statements", company=request.company_id)

    total_assets = request.balance_sheet.get("current_assets", 0) + request.balance_sheet.get("non_current_assets", 0)
    total_liabilities = request.balance_sheet.get("current_liabilities", 0) + request.balance_sheet.get(
        "non_current_liabilities", 0
    )
    equity = total_assets - total_liabilities

    revenue = request.income_statement.get("revenue", 0)
    net_income = request.income_statement.get("net_income", 0)
    ebit = request.income_statement.get("ebitda", 0) - request.income_statement.get("depreciation", 0)

    current_ratio = request.balance_sheet.get("current_assets", 1) / request.balance_sheet.get("current_liabilities", 1)
    quick_ratio = (
        request.balance_sheet.get("current_assets", 0) - request.balance_sheet.get("inventory", 0)
    ) / request.balance_sheet.get("current_liabilities", 1)
    roe = net_income / equity * 100 if equity else 0
    roa = net_income / total_assets * 100 if total_assets else 0
    profit_margin = net_income / revenue * 100 if revenue else 0

    key_ratios = {
        "current_ratio": round(current_ratio, 2),
        "quick_ratio": round(quick_ratio, 2),
        "debt_to_equity": round(total_liabilities / equity, 2) if equity else 0,
        "roe": round(roe, 2),
        "roa": round(roa, 2),
        "profit_margin": round(profit_margin, 2),
        "gross_margin": round((revenue - request.income_statement.get("cogs", 0)) / revenue * 100, 2) if revenue else 0,
    }

    health_score = min(
        100,
        (current_ratio / 2 * 25)
        + (profit_margin / 20 * 25)
        + ((100 - abs(key_ratios["debt_to_equity"])) * 0.25)
        + (key_ratios["roe"] / 30 * 25),
    )

    statements = {
        "balance_sheet": {"total_assets": total_assets, "total_liabilities": total_liabilities, "equity": equity},
        "income_statement": {"revenue": revenue, "net_income": net_income, "ebit": ebit},
        "cash_flow": request.cash_flow,
        "equity_statement": request.equity_statement,
    }

    recommendations = []
    if current_ratio < 1.5:
        recommendations.append("Low liquidity - improve working capital")
    if roe < 10:
        recommendations.append("Low return on equity - improve profitability")
    if key_ratios["debt_to_equity"] > 2:
        recommendations.append("High leverage - reduce debt levels")

    return FinancialStatementsResponse(
        company_id=request.company_id,
        period=request.period,
        statements=statements,
        key_ratios=key_ratios,
        financial_health_score=round(health_score, 2),
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8332)
