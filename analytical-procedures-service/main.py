"""
Analytical Procedures Service
Port: 8197
Ratio analysis, trend analysis, benchmarking
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Analytical Procedures Service", version="1.0.0")

class FinancialData(BaseModel):
    period: str
    revenue: float
    gross_profit: float
    operating_expenses: float
    net_income: float
    total_assets: float
    total_liabilities: float
    equity: float
    working_capital: float

class RatioComparison(BaseModel):
    ratio_name: str
    current_period: float
    prior_period: float
    industry_benchmark: float
    variance_pct: float
    status: str

class AnalyticalProceduresRequest(BaseModel):
    audit_id: str
    company_id: str
    current_data: FinancialData
    prior_period_data: FinancialData
    industry_benchmarks: Dict[str, float]
    non_financial_factors: List[str]

class AnalyticalProceduresResponse(BaseModel):
    audit_id: str
    ratio_analysis: List[RatioComparison]
    trend_indicators: Dict[str, str]
    unexpected_variances: List[Dict[str, Any]]
    analytical_findings: List[str]
    conclusion: str
    reliance_level: str

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
    return {"status": "healthy", "service": "analytical-procedures", "version": "1.0.0"}

@app.post("/analyze", response_model=AnalyticalProceduresResponse)
async def perform_analytical_procedures(request: AnalyticalProceduresRequest):
    logger.info("Performing analytical procedures", audit=request.audit_id, company=request.company_id)

    current = request.current_data
    prior = request.prior_period_data

    ratios = []

    def calc_ratio(curr: float, base: float) -> float:
        return round(curr / base, 4) if base else 0

    def get_variance(curr: float, prior: float) -> float:
        return round(((curr - prior) / prior * 100), 2) if prior else 0

    def check_status(variance: float, threshold: float = 20.0) -> str:
        if abs(variance) > threshold:
            return "significant_variance"
        return "within_expectations"

    ratios.append(RatioComparison(
        ratio_name="gross_margin",
        current_period=calc_ratio(current.gross_profit, current.revenue),
        prior_period=calc_ratio(prior.gross_profit, prior.revenue),
        industry_benchmark=request.industry_benchmarks.get("gross_margin", 0.35),
        variance_pct=get_variance(calc_ratio(current.gross_profit, current.revenue), calc_ratio(prior.gross_profit, prior.revenue)),
        status=check_status(get_variance(current.gross_profit / current.revenue, prior.gross_profit / prior.revenue) if current.revenue and prior.revenue else 0)
    ))

    ratios.append(RatioComparison(
        ratio_name="operating_margin",
        current_period=calc_ratio(current.operating_expenses, current.revenue),
        prior_period=calc_ratio(prior.operating_expenses, prior.revenue),
        industry_benchmark=request.industry_benchmarks.get("operating_margin", 0.15),
        variance_pct=get_variance(current.operating_expenses / current.revenue, prior.operating_expenses / prior.revenue) if current.revenue and prior.revenue else 0,
        status=check_status(get_variance(current.operating_expenses / current.revenue, prior.operating_expenses / prior.revenue) if current.revenue and prior.revenue else 0)
    ))

    ratios.append(RatioComparison(
        ratio_name="return_on_assets",
        current_period=calc_ratio(current.net_income, current.total_assets),
        prior_period=calc_ratio(prior.net_income, prior.total_assets),
        industry_benchmark=request.industry_benchmarks.get("roa", 0.08),
        variance_pct=get_variance(current.net_income / current.total_assets, prior.net_income / prior.total_assets) if current.total_assets and prior.total_assets else 0,
        status=check_status(get_variance(current.net_income / current.total_assets, prior.net_income / prior.total_assets) if current.total_assets and prior.total_assets else 0)
    ))

    ratios.append(RatioComparison(
        ratio_name="debt_to_equity",
        current_period=calc_ratio(current.total_liabilities, current.equity),
        prior_period=calc_ratio(prior.total_liabilities, prior.equity),
        industry_benchmark=request.industry_benchmarks.get("debt_equity", 1.5),
        variance_pct=get_variance(current.total_liabilities / current.equity, prior.total_liabilities / prior.equity) if current.equity and prior.equity else 0,
        status=check_status(get_variance(current.total_liabilities / current.equity, prior.total_liabilities / prior.equity) if current.equity and prior.equity else 0)
    ))

    unexpected_variances = [r for r in ratios if r.status == "significant_variance"]
    findings = [f"Unusual {r.ratio_name} variance of {r.variance_pct}%" for r in unexpected_variances]

    reliance = "low" if len(unexpected_variances) > 2 else "moderate" if len(unexpected_variances) > 0 else "high"

    return AnalyticalProceduresResponse(
        audit_id=request.audit_id,
        ratio_analysis=ratios,
        trend_indicators={
            "revenue_growth": "increasing" if current.revenue > prior.revenue else "decreasing",
            "profitability": "improving" if current.net_income > prior.net_income else "declining",
            "leverage": "increasing" if current.total_liabilities / current.equity > prior.total_liabilities / prior.equity else "decreasing"
        },
        unexpected_variances=[{"ratio": v.ratio_name, "variance": v.variance_pct} for v in unexpected_variances],
        analytical_findings=findings if findings else ["No significant unexpected variances identified"],
        conclusion="Analytical procedures completed with " + reliance + " reliance on substantive testing",
        reliance_level=reliance
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8197)
