"""
Working Capital Service
Port: 8237
Working capital analysis and optimization
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Working Capital Service", version="1.0.0")

class WorkingCapitalMetrics(BaseModel):
    current_ratio: float
    quick_ratio: float
    cash_ratio: float
    working_capital: float
    nwc: float
    operating_cycle_days: float

class WorkingCapitalRequest(BaseModel):
    company_id: str
    current_assets: float
    current_liabilities: float
    inventory: float
    accounts_receivable: float
    cash: float
    accounts_payable: float
    short_term_debt: float
    cost_of_goods_sold: float
    revenue: float

class WorkingCapitalResponse(BaseModel):
    company_id: str
    metrics: WorkingCapitalMetrics
    cash_conversion_cycle: float
    working_capital_turnover: float
    receivables_turnover: float
    payables_turnover: float
    inventory_turnover: float
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "working-capital", "version": "1.0.0"}

@app.post("/analyze", response_model=WorkingCapitalResponse)
async def analyze_working_capital(request: WorkingCapitalRequest):
    logger.info("Analyzing working capital", company=request.company_id)

    working_capital = request.current_assets - request.current_liabilities
    nwc = working_capital - request.cash
    current_ratio = request.current_assets / request.current_liabilities if request.current_liabilities else 0
    quick_ratio = (request.current_assets - request.inventory) / request.current_liabilities if request.current_liabilities else 0
    cash_ratio = request.cash / request.current_liabilities if request.current_liabilities else 0

    inv_turnover = request.cost_of_goods_sold / request.inventory if request.inventory else 0
    rec_turnover = request.revenue / request.accounts_receivable if request.accounts_receivable else 0
    pay_turnover = request.cost_of_goods_sold / request.accounts_payable if request.accounts_payable else 0

    inv_days = 365 / inv_turnover if inv_turnover else 0
    rec_days = 365 / rec_turnover if rec_turnover else 0
    pay_days = 365 / pay_turnover if pay_turnover else 0
    operating_cycle = inv_days + rec_days
    cash_conversion_cycle = operating_cycle - pay_days

    wc_turnover = request.revenue / working_capital if working_capital else 0

    metrics = WorkingCapitalMetrics(
        current_ratio=round(current_ratio, 4),
        quick_ratio=round(quick_ratio, 4),
        cash_ratio=round(cash_ratio, 4),
        working_capital=round(working_capital, 2),
        nwc=round(nwc, 2),
        operating_cycle_days=round(operating_cycle, 2)
    )

    recommendations = []
    if current_ratio < 1.5:
        recommendations.append("Current ratio below 1.5 - consider improving liquidity")
    if cash_conversion_cycle > 90:
        recommendations.append("Cash conversion cycle is high - focus on reducing inventory and receivables days")
    if quick_ratio < 1.0:
        recommendations.append("Quick ratio below 1.0 - improve immediate liquidity position")

    return WorkingCapitalResponse(
        company_id=request.company_id,
        metrics=metrics,
        cash_conversion_cycle=round(cash_conversion_cycle, 2),
        working_capital_turnover=round(wc_turnover, 4),
        receivables_turnover=round(rec_turnover, 4),
        payables_turnover=round(pay_turnover, 4),
        inventory_turnover=round(inv_turnover, 4),
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8237)
