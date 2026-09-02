"""
Vimbai Analytics Service
Financial ratio analysis, trend detection, and benchmarking.
Port: 8389
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "analytics-service"
PORT = int(os.getenv("PORT", "8389"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Analytics Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class FinancialData(BaseModel):
    revenue: float; net_income: float; total_assets: float
    current_assets: float; current_liabilities: float
    total_liabilities: float; total_equity: float
    inventory: float = 0; accounts_receivable: float = 0
    cogs: float = 0; operating_cash_flow: float = 0
    shares_outstanding: int = 0

class BenchmarkData(BaseModel):
    industry_avg_roe: float = 0.15; industry_avg_current_ratio: float = 1.8
    industry_avg_debt_to_equity: float = 0.6; industry_avg_net_margin: float = 0.10
    industry_avg_asset_turnover: float = 0.8

class AnalysisRequest(BaseModel):
    company_id: str; period: str
    financials: FinancialData; benchmark: BenchmarkData = BenchmarkData()

class AnalysisResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; period: str
    profitability: Dict[str, float]
    liquidity: Dict[str, float]
    solvency: Dict[str, float]
    efficiency: Dict[str, float]
    benchmark_comparison: Dict[str, Dict[str, float]]
    overall_score: float
    insights: List[str] = []

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/analyze", response_model=AnalysisResult)
async def analyze(req: AnalysisRequest):
    f = req.financials
    
    profitability = {
        "gross_margin": round(f.net_income / f.revenue * 100, 2) if f.revenue else 0,
        "net_margin": round(f.net_income / f.revenue * 100, 2) if f.revenue else 0,
        "roe": round(f.net_income / f.total_equity * 100, 2) if f.total_equity else 0,
        "roa": round(f.net_income / f.total_assets * 100, 2) if f.total_assets else 0,
    }
    
    liquidity = {
        "current_ratio": round(f.current_assets / f.current_liabilities, 2) if f.current_liabilities else 0,
        "quick_ratio": round((f.current_assets - f.inventory) / f.current_liabilities, 2) if f.current_liabilities else 0,
        "cash_ratio": round(f.operating_cash_flow / f.current_liabilities, 2) if f.current_liabilities else 0,
    }
    
    solvency = {
        "debt_to_equity": round(f.total_liabilities / f.total_equity, 2) if f.total_equity else 0,
        "debt_to_assets": round(f.total_liabilities / f.total_assets * 100, 2) if f.total_assets else 0,
        "interest_coverage": round(f.net_income / max(f.total_liabilities * 0.05, 1), 2) if f.total_liabilities else 0,
    }
    
    efficiency = {
        "asset_turnover": round(f.revenue / f.total_assets, 2) if f.total_assets else 0,
        "inventory_turnover": round(f.cogs / f.inventory, 2) if f.inventory else 0,
        "receivables_turnover": round(f.revenue / f.accounts_receivable, 2) if f.accounts_receivable else 0,
        "dso": round(f.accounts_receivable / f.revenue * 365, 1) if f.revenue else 0,
    }
    
    benchmark = {
        "roe": {"company": profitability["roe"], "industry": req.benchmark.industry_avg_roe * 100,
                "above_industry": profitability["roe"] > req.benchmark.industry_avg_roe * 100},
        "current_ratio": {"company": liquidity["current_ratio"], "industry": req.benchmark.industry_avg_current_ratio,
                          "above_industry": liquidity["current_ratio"] > req.benchmark.industry_avg_current_ratio},
        "debt_to_equity": {"company": solvency["debt_to_equity"], "industry": req.benchmark.industry_avg_debt_to_equity,
                           "above_industry": solvency["debt_to_equity"] < req.benchmark.industry_avg_debt_to_equity},
        "net_margin": {"company": profitability["net_margin"], "industry": req.benchmark.industry_avg_net_margin * 100,
                       "above_industry": profitability["net_margin"] > req.benchmark.industry_avg_net_margin * 100},
    }
    
    score = 0
    for v in benchmark.values():
        if v["above_industry"]:
            score += 25
    score = round(score, 1)
    
    insights = []
    if profitability["roe"] < req.benchmark.industry_avg_roe * 100:
        insights.append("ROE below industry average - consider equity efficiency improvements")
    if liquidity["current_ratio"] < 1:
        insights.append("Current ratio below 1.0 - potential short-term liquidity risk")
    if solvency["debt_to_equity"] > 1:
        insights.append("High leverage - debt-to-equity ratio above 1.0")
    if efficiency["dso"] > 60:
        insights.append(f"DSO of {efficiency['dso']} days suggests collection process needs improvement")
    if not insights:
        insights.append("All key metrics within healthy ranges")
    
    return AnalysisResult(
        company_id=req.company_id, period=req.period,
        profitability=profitability, liquidity=liquidity,
        solvency=solvency, efficiency=efficiency,
        benchmark_comparison=benchmark, overall_score=score,
        insights=insights
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
