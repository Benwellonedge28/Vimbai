"""
Debt Covenants Service
Port: 8240
Debt covenant monitoring and compliance
"""

from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Debt Covenants Service", version="1.0.0")


class CovenantMetric(BaseModel):
    name: str
    actual_value: float
    threshold: float
    operator: str
    compliant: bool
    headroom: float
    headroom_percentage: float


class Covenant(BaseModel):
    covenant_id: str
    covenant_type: str
    description: str
    metrics: List[CovenantMetric]


class DebtCovenantRequest(BaseModel):
    company_id: str
    total_debt: float
    ebitda: float
    interest_expense: float
    current_assets: float
    current_liabilities: float
    tangible_net_worth: float
    total_assets: float
    equity: float
    net_income: float
    revenue: float


class DebtCovenantResponse(BaseModel):
    company_id: str
    analysis_date: str
    covenants: List[Covenant]
    overall_compliance: bool
    covenant_count: int
    compliant_count: int
    breached_count: int
    warning_count: int
    breach_risk: str
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "debt-covenants", "version": "1.0.0"}


@app.post("/analyze", response_model=DebtCovenantResponse)
async def analyze_debt_covenants(request: DebtCovenantRequest):
    logger.info("Analyzing debt covenants", company=request.company_id)

    debt_to_equity = request.total_debt / request.equity if request.equity else 0
    debt_to_ebitda = request.total_debt / request.ebitda if request.ebitda else 0
    interest_coverage = request.ebitda / request.interest_expense if request.interest_expense else 0
    current_ratio = request.current_assets / request.current_liabilities if request.current_liabilities else 0
    debt_to_assets = request.total_debt / request.total_assets if request.total_assets else 0
    tangible_net_worth_ratio = request.tangible_net_worth / request.total_assets if request.total_assets else 0
    net_margin = request.net_income / request.revenue if request.revenue else 0

    covenants = []
    all_compliant = True
    warning_count = 0

    def check_covenant(name, actual, threshold, operator, covenant_type):
        if operator == "<=":
            compliant = actual <= threshold
            headroom = threshold - actual
        elif operator == ">=":
            compliant = actual >= threshold
            headroom = actual - threshold
        elif operator == "<":
            compliant = actual < threshold
            headroom = threshold - actual
        else:
            compliant = actual > threshold
            headroom = actual - threshold

        headroom_pct = (headroom / threshold * 100) if threshold else 0
        return CovenantMetric(
            name=name,
            actual_value=round(actual, 4),
            threshold=threshold,
            operator=operator,
            compliant=compliant,
            headroom=round(headroom, 4),
            headroom_percentage=round(headroom_pct, 2),
        )

    covenants.append(
        Covenant(
            covenant_id="LEV-001",
            covenant_type="Leverage",
            description="Debt to Equity ratio covenant",
            metrics=[check_covenant("Debt/Equity", debt_to_equity, 2.0, "<=", "Leverage")],
        )
    )
    covenants.append(
        Covenant(
            covenant_id="LEV-002",
            covenant_type="Leverage",
            description="Debt to EBITDA ratio covenant",
            metrics=[check_covenant("Debt/EBITDA", debt_to_ebitda, 4.0, "<=", "Leverage")],
        )
    )
    covenants.append(
        Covenant(
            covenant_id="COV-001",
            covenant_type="Coverage",
            description="Interest coverage ratio covenant",
            metrics=[check_covenant("Interest Coverage", interest_coverage, 2.5, ">=", "Coverage")],
        )
    )
    covenants.append(
        Covenant(
            covenant_id="LIQ-001",
            covenant_type="Liquidity",
            description="Current ratio covenant",
            metrics=[check_covenant("Current Ratio", current_ratio, 1.2, ">=", "Liquidity")],
        )
    )
    covenants.append(
        Covenant(
            covenant_id="TNW-001",
            covenant_type="Net Worth",
            description="Minimum tangible net worth",
            metrics=[check_covenant("TNW/Total Assets", tangible_net_worth_ratio, 0.15, ">=", "Net Worth")],
        )
    )

    for c in covenants:
        for m in c.metrics:
            if not m.compliant:
                all_compliant = False
            if m.headroom_percentage < 10:
                warning_count += 1

    breached = sum(1 for c in covenants for m in c.metrics if not m.compliant)

    if breached > 0:
        breach_risk = "HIGH"
    elif warning_count > 2:
        breach_risk = "MEDIUM"
    else:
        breach_risk = "LOW"

    recommendations = []
    if breached > 0:
        recommendations.append("URGENT: Covenant breach detected - engage with lenders immediately")
    if debt_to_ebitda > 3.5:
        recommendations.append("Approaching leverage limit - consider debt reduction strategies")
    if interest_coverage < 3.0:
        recommendations.append("Interest coverage declining - monitor closely and consider refinancing")

    return DebtCovenantResponse(
        company_id=request.company_id,
        analysis_date=datetime.now().isoformat(),
        covenants=covenants,
        overall_compliance=all_compliant,
        covenant_count=len(covenants),
        compliant_count=len(covenants) - breached,
        breached_count=breached,
        warning_count=warning_count,
        breach_risk=breach_risk,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8240)
