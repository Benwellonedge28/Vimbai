"""
Risk Return Analysis Service
Port: 8236
Risk-adjusted return metrics
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Risk Return Analysis Service", version="1.0.0")


class RiskReturnMetrics(BaseModel):
    asset_id: str
    asset_name: str
    expected_return: float
    standard_deviation: float
    variance: float
    sharpe_ratio: float
    sortino_ratio: float
    treynor_ratio: float
    beta: float
    alpha: float
    information_ratio: float


class RiskReturnRequest(BaseModel):
    company_id: str
    assets: List[Dict[str, Any]]
    benchmark_return: float
    risk_free_rate: float
    returns_data: List[List[float]]


class RiskReturnResponse(BaseModel):
    company_id: str
    risk_return_metrics: List[RiskReturnMetrics]
    portfolio_expected_return: float
    portfolio_risk: float
    portfolio_sharpe_ratio: float
    efficient_frontier: List[Dict[str, float]]
    recommendations: List[str]


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
    return {"status": "healthy", "service": "risk-return-analysis", "version": "1.0.0"}


@app.post("/analyze", response_model=RiskReturnResponse)
async def analyze_risk_return(request: RiskReturnRequest):
    logger.info("Analyzing risk return", company=request.company_id)

    risk_return_metrics = []

    for i, asset in enumerate(request.assets):
        returns = request.returns_data[i] if i < len(request.returns_data) else []
        exp_return = sum(returns) / len(returns) if returns else asset.get("expected_return", 0.1)
        variance = sum((r - exp_return) ** 2 for r in returns) / len(returns) if returns else 0.02
        std_dev = variance**0.5

        downside_returns = [r for r in returns if r < 0]
        downside_variance = (
            sum(r**2 for r in downside_returns) / len(downside_returns) if downside_returns else variance
        )
        sortino = (exp_return - request.risk_free_rate) / (downside_variance**0.5) if downside_variance else 0

        sharpe = (exp_return - request.risk_free_rate) / std_dev if std_dev else 0
        beta = asset.get("beta", 1.0)
        treynor = (exp_return - request.risk_free_rate) / beta if beta else 0
        alpha = exp_return - (request.risk_free_rate + beta * (request.benchmark_return - request.risk_free_rate))

        risk_return_metrics.append(
            RiskReturnMetrics(
                asset_id=asset.get("id", ""),
                asset_name=asset.get("name", ""),
                expected_return=round(exp_return, 4),
                standard_deviation=round(std_dev, 4),
                variance=round(variance, 6),
                sharpe_ratio=round(sharpe, 4),
                sortino_ratio=round(sortino, 4),
                treynor_ratio=round(treynor, 4),
                beta=round(beta, 4),
                alpha=round(alpha, 4),
                information_ratio=round(alpha / 0.1, 4),
            )
        )

    portfolio_return = sum(m.expected_return for m in risk_return_metrics) / len(risk_return_metrics)
    portfolio_risk = sum(m.standard_deviation for m in risk_return_metrics) / len(risk_return_metrics) * 0.8
    portfolio_sharpe = (portfolio_return - request.risk_free_rate) / portfolio_risk if portfolio_risk else 0

    return RiskReturnResponse(
        company_id=request.company_id,
        risk_return_metrics=risk_return_metrics,
        portfolio_expected_return=round(portfolio_return, 4),
        portfolio_risk=round(portfolio_risk, 4),
        portfolio_sharpe_ratio=round(portfolio_sharpe, 4),
        efficient_frontier=[
            {"return": 0.05, "risk": 0.05},
            {"return": 0.08, "risk": 0.08},
            {"return": 0.12, "risk": 0.12},
            {"return": 0.15, "risk": 0.16},
        ],
        recommendations=[
            "Consider assets with higher Sharpe ratios",
            "Monitor Sortino ratio for downside risk",
            "Review beta against market movements",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8236)
