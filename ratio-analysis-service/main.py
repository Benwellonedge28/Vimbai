"""
Ratio Analysis Service
Port: 8348
Financial ratio calculations and analysis
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from datetime import datetime
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Ratio Analysis Service", version="1.0.0")

class FinancialRatiosRequest(BaseModel):
    company_id: str
    period: str
    balance_sheet: Dict[str, float]
    income_statement: Dict[str, float]
    cash_flow: Dict[str, float]

class FinancialRatiosResponse(BaseModel):
    company_id: str
    period: str
    liquidity_ratios: Dict[str, float]
    profitability_ratios: Dict[str, float]
    leverage_ratios: Dict[str, float]
    efficiency_ratios: Dict[str, float]
    valuation_ratios: Dict[str, float]
    overall_score: float
    recommendations: List[str]

class PeerComparisonRequest(BaseModel):
    company_id: str
    industry: str
    company_ratios: Dict[str, float]
    peer_ratios: List[Dict[str, float]]

class PeerComparisonResponse(BaseModel):
    company_id: str
    industry: str
    percentile_rankings: Dict[str, float]
    strengths: List[str]
    weaknesses: List[str]
    improvement_areas: List[Dict[str, Any]]

class TrendAnalysisRequest(BaseModel):
    company_id: str
    periods: List[str]
    ratio_history: Dict[str, List[float]]

class TrendAnalysisResponse(BaseModel):
    company_id: str
    trend_summary: Dict[str, Dict[str, Any]]
    momentum: Dict[str, str]
    predictions: Dict[str, float]
    alerts: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "ratio-analysis", "version": "1.0.0"}

@app.post("/ratios", response_model=FinancialRatiosResponse)
async def calculate_ratios(request: FinancialRatiosRequest):
    logger.info("Calculating financial ratios", company=request.company_id, period=request.period)

    bs = request.balance_sheet
    is_ = request.income_statement

    current_assets = bs.get("current_assets", 0)
    current_liabilities = bs.get("current_liabilities", 0)
    total_assets = bs.get("total_assets", 1)
    total_debt = bs.get("total_debt", 0)
    equity = bs.get("total_equity", 0)
    revenue = is_.get("revenue", 0)
    net_income = is_.get("net_income", 0)
    ebit = is_.get("ebit", net_income)
    cogs = is_.get("cogs", 0)
    inventory = bs.get("inventory", 0)
    ar = bs.get("accounts_receivable", 0)
    ap = bs.get("accounts_payable", 0)

    liquidity = {
        "current_ratio": round(current_assets / current_liabilities if current_liabilities else 0, 4),
        "quick_ratio": round((current_assets - inventory) / current_liabilities if current_liabilities else 0, 4),
        "cash_ratio": round((bs.get("cash", 0) + bs.get("marketable_securities", 0)) / current_liabilities if current_liabilities else 0, 4),
        "working_capital": round(current_assets - current_liabilities, 2)
    }

    profitability = {
        "gross_margin": round((revenue - cogs) / revenue * 100 if revenue else 0, 2),
        "operating_margin": round(ebit / revenue * 100 if revenue else 0, 2),
        "net_margin": round(net_income / revenue * 100 if revenue else 0, 2),
        "roe": round(net_income / equity * 100 if equity else 0, 2),
        "roa": round(net_income / total_assets * 100 if total_assets else 0, 2),
        "roce": round(ebit / (total_assets - current_liabilities) * 100 if total_assets else 0, 2)
    }

    leverage = {
        "debt_to_equity": round(total_debt / equity if equity else 0, 4),
        "debt_to_assets": round(total_debt / total_assets if total_assets else 0, 4),
        "interest_coverage": round(ebit / (is_.get("interest_expense", 1) or 1), 2),
        "equity_multiplier": round(total_assets / equity if equity else 0, 4)
    }

    efficiency = {
        "asset_turnover": round(revenue / total_assets if total_assets else 0, 4),
        "inventory_turnover": round(cogs / inventory if inventory else 0, 4),
        "ar_turnover": round(revenue / ar if ar else 0, 4),
        "ap_turnover": round(cogs / ap if ap else 0, 4),
        "days_sales_outstanding": round(ar / revenue * 365 if revenue else 0, 2)
    }

    valuation = {
        "pe_ratio": round(bs.get("market_cap", equity * 3) / net_income if net_income else 0, 2),
        "pb_ratio": round(bs.get("market_cap", equity * 3) / equity if equity else 0, 2),
        "ps_ratio": round(bs.get("market_cap", equity * 3) / revenue if revenue else 0, 2)
    }

    overall_score = sum([
        min(liquidity["current_ratio"] / 2, 100),
        min(profitability["net_margin"] + 50, 100),
        min(100 - leverage["debt_to_equity"] * 20, 100),
        min(efficiency["asset_turnover"] * 50, 100)
    ]) / 4

    recommendations = []
    if liquidity["current_ratio"] < 1.5:
        recommendations.append("Improve liquidity position")
    if profitability["net_margin"] < 10:
        recommendations.append("Focus on cost reduction")
    if leverage["debt_to_equity"] > 2:
        recommendations.append("Consider debt reduction")

    return FinancialRatiosResponse(
        company_id=request.company_id,
        period=request.period,
        liquidity_ratios=liquidity,
        profitability_ratios=profitability,
        leverage_ratios=leverage,
        efficiency_ratios=efficiency,
        valuation_ratios=valuation,
        overall_score=round(overall_score, 2),
        recommendations=recommendations
    )

@app.post("/peer-comparison", response_model=PeerComparisonResponse)
async def compare_with_peers(request: PeerComparisonRequest):
    logger.info("Comparing with peers", company=request.company_id, industry=request.industry)

    percentile = {}
    for ratio, value in request.company_ratios.items():
        values = [p.get(ratio, 0) for p in request.peer_ratios]
        values.append(value)
        values.sort()
        pct = values.index(value) / len(values) * 100
        percentile[ratio] = round(pct, 2)

    strengths = [k for k, v in percentile.items() if v >= 70]
    weaknesses = [k for k, v in percentile.items() if v < 30]

    return PeerComparisonResponse(
        company_id=request.company_id,
        industry=request.industry,
        percentile_rankings=percentile,
        strengths=strengths,
        weaknesses=weaknesses,
        improvement_areas=[{"ratio": w, "industry_avg": 50} for w in weaknesses]
    )

@app.post("/trend", response_model=TrendAnalysisResponse)
async def analyze_trends(request: TrendAnalysisRequest):
    logger.info("Analyzing ratio trends", company=request.company_id, periods=len(request.periods))

    trend_summary = {}
    momentum = {}
    predictions = {}
    alerts = []

    for ratio, values in request.ratio_history.items():
        if len(values) >= 2:
            change = values[-1] - values[0]
            pct_change = change / values[0] * 100 if values[0] else 0
            trend_summary[ratio] = {
                "change": round(change, 4),
                "pct_change": round(pct_change, 2),
                "current": values[-1],
                "average": round(sum(values) / len(values), 4)
            }
            momentum[ratio] = "improving" if change > 0 else "declining"
            predictions[ratio] = round(values[-1] * 1.02, 4)
        else:
            trend_summary[ratio] = {"current": values[0] if values else 0}
            momentum[ratio] = "stable"

    return TrendAnalysisResponse(
        company_id=request.company_id,
        trend_summary=trend_summary,
        momentum=momentum,
        predictions=predictions,
        alerts=alerts
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8348)
