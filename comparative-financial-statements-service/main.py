"""
Comparative Financial Statements Service
Port: 8142
Creates comparative statements with horizontal analysis and trend percentages
"""
import httpx
import structlog
from datetime import datetime
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI, HTTPException

logger = structlog.get_logger()
app = FastAPI(title="Comparative Financial Statements Service", version="1.0.0")

# Pydantic Models
class PeriodData(BaseModel):
    period: str
    revenue: float
    cost_of_sales: float
    gross_profit: float
    operating_expenses: float
    operating_profit: float
    net_profit: float
    total_assets: float
    total_liabilities: float
    total_equity: float
    cash_from_operations: float

class HorizontalAnalysisItem(BaseModel):
    item_name: str
    current_period: float
    prior_period: float
    change: float
    change_percentage: float
    absolute_change: float

class TrendAnalysisItem(BaseModel):
    item_name: str
    base_period_value: float
    periods: List[Dict[str, float]]
    trend_percentages: List[float]
    compound_annual_growth_rate: float

class RatioComparison(BaseModel):
    ratio_name: str
    current_value: float
    prior_value: float
    change: float
    industry_benchmark: Optional[float] = None

class ComparativeRequest(BaseModel):
    company_id: str
    current_period: str
    prior_period: str
    base_period: Optional[str] = None
    include_trend: bool = True
    include_ratio_comparison: bool = True
    number_of_periods: int = Field(default=5, ge=3, le=10)

class ComparativeResponse(BaseModel):
    company_id: str
    periods: List[str]
    horizontal_analysis: List[HorizontalAnalysisItem]
    trend_analysis: Optional[List[TrendAnalysisItem]] = None
    ratio_comparison: Optional[List[RatioComparison]] = None
    summary_statistics: Dict[str, Any]

async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    """Call another internal FinAcc service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            if data:
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "comparative-financial-statements", "version": "1.0.0"}

@app.post("/analyze", response_model=ComparativeResponse)
async def analyze_comparative_statements(request: ComparativeRequest):
    """Perform comprehensive comparative financial statement analysis."""
    logger.info("Analyzing comparative statements", company=request.company_id, periods=request.number_of_periods)

    # Generate period data (simulated historical data)
    base_revenue = 10000000.0
    periods = []
    for i in range(request.number_of_periods):
        growth = 1 + (i * 0.05)  # 5% growth per period
        period_data = {
            "period": f"Period {i+1}",
            "revenue": base_revenue * growth,
            "cost_of_sales": base_revenue * growth * 0.55,
            "gross_profit": base_revenue * growth * 0.45,
            "operating_expenses": base_revenue * growth * 0.20,
            "operating_profit": base_revenue * growth * 0.25,
            "net_profit": base_revenue * growth * 0.18,
            "total_assets": base_revenue * growth * 4,
            "total_liabilities": base_revenue * growth * 2.2,
            "total_equity": base_revenue * growth * 1.8,
            "cash_from_operations": base_revenue * growth * 0.22
        }
        periods.append(period_data)

    # Horizontal analysis
    current = periods[-1]
    prior = periods[-2]

    horizontal_items = [
        ("Revenue", current["revenue"], prior["revenue"]),
        ("Cost of Sales", current["cost_of_sales"], prior["cost_of_sales"]),
        ("Gross Profit", current["gross_profit"], prior["gross_profit"]),
        ("Operating Expenses", current["operating_expenses"], prior["operating_expenses"]),
        ("Operating Profit", current["operating_profit"], prior["operating_profit"]),
        ("Net Profit", current["net_profit"], prior["net_profit"]),
        ("Total Assets", current["total_assets"], prior["total_assets"]),
        ("Total Liabilities", current["total_liabilities"], prior["total_liabilities"]),
        ("Total Equity", current["total_equity"], prior["total_equity"]),
        ("Cash from Operations", current["cash_from_operations"], prior["cash_from_operations"])
    ]

    horizontal_analysis = []
    for name, curr, prev in horizontal_items:
        change = curr - prev
        change_pct = (change / prev * 100) if prev != 0 else 0
        horizontal_analysis.append(HorizontalAnalysisItem(
            item_name=name,
            current_period=curr,
            prior_period=prev,
            change=change,
            change_percentage=change_pct,
            absolute_change=abs(change)
        ))

    # Trend analysis
    trend_analysis = None
    if request.include_trend:
        trend_analysis = []
        base_values = {
            "Revenue": periods[0]["revenue"],
            "Net Profit": periods[0]["net_profit"],
            "Total Assets": periods[0]["total_assets"],
            "Operating Profit": periods[0]["operating_profit"]
        }

        for name, base_value in base_values.items():
            trend_pcts = [(p[name] / base_value * 100) for p in periods]
            # Calculate CAGR
            final_value = periods[-1][name]
            n_periods = len(periods) - 1
            cagr = ((final_value / base_value) ** (1/n_periods) - 1) * 100 if base_value > 0 else 0

            trend_analysis.append(TrendAnalysisItem(
                item_name=name,
                base_period_value=base_value,
                periods=[{"period": p["period"], "value": p[name]} for p in periods],
                trend_percentages=trend_pcts,
                compound_annual_growth_rate=cagr
            ))

    # Ratio comparison
    ratio_comparison = None
    if request.include_ratio_comparison:
        # Calculate ratios for current and prior
        current_gross_margin = current["gross_profit"] / current["revenue"]
        prior_gross_margin = prior["gross_profit"] / prior["revenue"]

        current_op_margin = current["operating_profit"] / current["revenue"]
        prior_op_margin = prior["operating_profit"] / prior["revenue"]

        current_roa = current["net_profit"] / current["total_assets"]
        prior_roa = prior["net_profit"] / prior["total_assets"]

        current_debt_ratio = current["total_liabilities"] / current["total_assets"]
        prior_debt_ratio = prior["total_liabilities"] / prior["total_assets"]

        ratio_comparison = [
            RatioComparison(
                ratio_name="Gross Margin",
                current_value=current_gross_margin * 100,
                prior_value=prior_gross_margin * 100,
                change=(current_gross_margin - prior_gross_margin) * 100,
                industry_benchmark=45.0
            ),
            RatioComparison(
                ratio_name="Operating Margin",
                current_value=current_op_margin * 100,
                prior_value=prior_op_margin * 100,
                change=(current_op_margin - prior_op_margin) * 100,
                industry_benchmark=20.0
            ),
            RatioComparison(
                ratio_name="Return on Assets",
                current_value=current_roa * 100,
                prior_value=prior_roa * 100,
                change=(current_roa - prior_roa) * 100,
                industry_benchmark=8.0
            ),
            RatioComparison(
                ratio_name="Debt Ratio",
                current_value=current_debt_ratio * 100,
                prior_value=prior_debt_ratio * 100,
                change=(current_debt_ratio - prior_debt_ratio) * 100,
                industry_benchmark=55.0
            )
        ]

    # Summary statistics
    summary = {
        "average_revenue_growth": sum((periods[i]["revenue"] - periods[i-1]["revenue"]) / periods[i-1]["revenue"]
                                       for i in range(1, len(periods))) / (len(periods) - 1) * 100,
        "average_profit_margin": sum(p["net_profit"] / p["revenue"] for p in periods) / len(periods) * 100,
        "total_asset_growth": (periods[-1]["total_assets"] - periods[0]["total_assets"]) / periods[0]["total_assets"] * 100,
        "equity_compound_growth": ((periods[-1]["total_equity"] / periods[0]["total_equity"]) ** (1/(len(periods)-1)) - 1) * 100
    }

    response = ComparativeResponse(
        company_id=request.company_id,
        periods=[p["period"] for p in periods],
        horizontal_analysis=horizontal_analysis,
        trend_analysis=trend_analysis,
        ratio_comparison=ratio_comparison,
        summary_statistics=summary
    )

    logger.info("Comparative analysis complete", company=request.company_id)
    return response

@app.post("/horizontal-analysis")
async def perform_horizontal_analysis(current_value: float, prior_value: float, item_name: str):
    """Perform horizontal analysis on a single item."""
    change = current_value - prior_value
    change_pct = (change / prior_value * 100) if prior_value != 0 else 0

    return {
        "item_name": item_name,
        "current_period": current_value,
        "prior_period": prior_value,
        "absolute_change": change,
        "percentage_change": change_pct,
        "interpretation": "Favorable" if change > 0 else "Unfavorable" if change < 0 else "No change"
    }

@app.post("/trend-analysis")
async def perform_trend_analysis(values: List[float], periods: List[str]):
    """Perform trend analysis on a series of values."""
    if not values or len(values) < 2:
        raise HTTPException(status_code=400, detail="At least 2 periods required")

    base_value = values[0]
    trend_percentages = [(v / base_value * 100) for v in values]

    # Calculate CAGR
    n_periods = len(values) - 1
    cagr = ((values[-1] / base_value) ** (1/n_periods) - 1) * 100 if base_value > 0 else 0

    return {
        "base_period": periods[0],
        "base_value": base_value,
        "final_period": periods[-1],
        "final_value": values[-1],
        "trend_percentages": [{"period": p, "value": v, "trend": t}
                              for p, v, t in zip(periods, values, trend_percentages)],
        "compound_annual_growth_rate": cagr,
        "average_growth": sum((values[i] - values[i-1]) / values[i-1] for i in range(1, len(values))) / (len(values) - 1) * 100
    }

@app.post("/variance-analysis")
async def variance_analysis(budgeted: List[float], actual: List[float], items: List[str]):
    """Perform variance analysis between budgeted and actual figures."""
    variances = []
    for i, item in enumerate(items):
        if i < len(budgeted) and i < len(actual):
            variance = actual[i] - budgeted[i]
            variance_pct = (variance / budgeted[i] * 100) if budgeted[i] != 0 else 0
            variances.append({
                "item": item,
                "budgeted": budgeted[i],
                "actual": actual[i],
                "variance": variance,
                "variance_percentage": variance_pct,
                "favorable": variance > 0
            })

    return {
        "variances": variances,
        "total_favorable_variance": sum(v["variance"] for v in variances if v["favorable"]),
        "total_unfavorable_variance": sum(abs(v["variance"]) for v in variances if not v["favorable"])
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8142)
