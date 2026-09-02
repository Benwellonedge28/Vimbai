"""
Vimbai Continue/Shutdown Decision Service
Helps decide whether to continue or shutdown a department/product.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "continue-shutdown-decision-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8074"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Continue/Shutdown Decision Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class CostAnalysis(BaseModel):
    # If continuing
    revenue: float = 0
    variable_costs: float = 0
    contribution: float = 0
    avoidable_fixed_costs: float = 0  # Fixed costs saved if shutdown
    unavoidable_fixed_costs: float = 0  # Costs that continue even if shutdown


class ContinueShutdownDecision(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_id: str  # Department or product ID
    entity_name: str
    entity_type: str  # department, product, division
    period: str
    cost_analysis: CostAnalysis
    shutdown_costs: float = 0  # Additional costs from shutdown (redundancy, etc.)
    shutdown_savings: float = 0  # Savings from shutdown
    net_shutdown_benefit: float = 0
    contribution_if_continue: float = 0
    financial_outcome_continue: float = 0  # Profit if continue
    financial_outcome_shutdown: float = 0  # Profit if shutdown
    recommendation: str = ""
    created_at: datetime = Field(default_factory=datetime.utcnow)


decisions: List[ContinueShutdownDecision] = []


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception:
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Continue/Shutdown decision analysis"}


@app.post("/analyze")
async def analyze_continue_shutdown(
    entity_id: str,
    entity_name: str,
    entity_type: str,
    period: str,
    revenue: float,
    variable_costs: float,
    avoidable_fixed_costs: float,
    unavoidable_fixed_costs: float,
    shutdown_costs: float = 0,
    shutdown_savings: float = 0,
):
    """Analyze continue vs shutdown decision."""
    cost_analysis = CostAnalysis(
        revenue=revenue,
        variable_costs=variable_costs,
        avoidable_fixed_costs=avoidable_fixed_costs,
        unavoidable_fixed_costs=unavoidable_fixed_costs,
    )

    cost_analysis.contribution = revenue - variable_costs

    # Calculate financial outcomes
    # If continue: Revenue - All Costs
    cost_analysis.avoidable_fixed_costs = avoidable_fixed_costs
    cost_analysis.unavoidable_fixed_costs = unavoidable_fixed_costs

    contribution_if_continue = revenue - variable_costs
    financial_outcome_continue = contribution_if_continue - unavoidable_fixed_costs

    # If shutdown: Save avoidable costs but incur shutdown costs
    net_shutdown_benefit = avoidable_fixed_costs - shutdown_costs + shutdown_savings
    financial_outcome_shutdown = -unavoidable_fixed_costs + net_shutdown_benefit

    # Make recommendation
    if financial_outcome_continue > financial_outcome_shutdown:
        recommendation = "CONTINUE"
    elif financial_outcome_shutdown > financial_outcome_continue:
        recommendation = "SHUTDOWN"
    else:
        recommendation = "INDIFFERENT"

    decision = ContinueShutdownDecision(
        entity_id=entity_id,
        entity_name=entity_name,
        entity_type=entity_type,
        period=period,
        cost_analysis=cost_analysis,
        shutdown_costs=shutdown_costs,
        shutdown_savings=shutdown_savings,
        net_shutdown_benefit=net_shutdown_benefit,
        contribution_if_continue=contribution_if_continue,
        financial_outcome_continue=financial_outcome_continue,
        financial_outcome_shutdown=financial_outcome_shutdown,
        recommendation=recommendation,
    )

    decisions.append(decision)
    return decision


@app.post("/quick-analysis")
async def quick_shutdown_analysis(entity_name: str, contribution: float, unavoidable_fixed_costs: float):
    """Quick analysis when only key figures are available."""
    financial_outcome = contribution - unavoidable_fixed_costs

    if contribution > unavoidable_fixed_costs:
        recommendation = "CONTINUE"
        reason = f"Contribution ({contribution}) exceeds unavoidable costs ({unavoidable_fixed_costs})"
    elif contribution < unavoidable_fixed_costs:
        recommendation = "CONSIDER SHUTDOWN"
        reason = f"Contribution ({contribution}) is less than unavoidable costs ({unavoidable_fixed_costs})"
    else:
        recommendation = "INDIFFERENT"
        reason = "Contribution equals unavoidable costs"

    return {
        "entity_name": entity_name,
        "contribution": contribution,
        "unavoidable_fixed_costs": unavoidable_fixed_costs,
        "financial_outcome": financial_outcome,
        "recommendation": recommendation,
        "reason": reason,
    }


@app.get("/decisions")
async def list_decisions(entity_id: Optional[str] = None, entity_type: Optional[str] = None):
    """List decisions."""
    result = decisions
    if entity_id:
        result = [d for d in result if d.entity_id == entity_id]
    if entity_type:
        result = [d for d in result if d.entity_type == entity_type]
    return {"decisions": result}


@app.get("/decisions/{decision_id}")
async def get_decision(decision_id: str):
    """Get decision details."""
    decision = next((d for d in decisions if d.id == decision_id), None)
    if not decision:
        return {"error": "Decision not found"}
    return decision


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
