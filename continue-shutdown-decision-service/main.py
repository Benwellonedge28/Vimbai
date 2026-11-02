"""
Vimbai Continue/Shutdown Decision Service
Helps decide whether to continue or shutdown a department/product.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "continue_shutdown_decision_service" not in _sys.modules or not hasattr(
    _sys.modules.get("continue_shutdown_decision_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "continue_shutdown_decision_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["continue_shutdown_decision_service"] = _pkg
    _sys.modules["continue_shutdown_decision_service"].__path__ = [_HERE]

import os
from typing import Any, Dict, List, Optional

import httpx
import structlog
from continue_shutdown_decision_service import crud
from continue_shutdown_decision_service.dependencies import book_id_var, get_db_session, get_user_id
from continue_shutdown_decision_service.models import ContinueShutdownDecision, CostAnalysis
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


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
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
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

    return await crud.create(session, user_id, decision)


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
async def list_decisions(
    entity_id: Optional[str] = None,
    entity_type: Optional[str] = None,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List decisions."""
    result = await crud.list_all(session, user_id, ContinueShutdownDecision)
    if entity_id:
        result = [d for d in result if d.entity_id == entity_id]
    if entity_type:
        result = [d for d in result if d.entity_type == entity_type]
    return {"decisions": result}


@app.get("/decisions/{decision_id}")
async def get_decision(
    decision_id: str,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Get decision details.

    The original answered 200 with an {"error": ...} body; that contract is kept.
    """
    decision = await crud.find(session, user_id, ContinueShutdownDecision, decision_id)
    if decision is None:
        return {"error": "Decision not found"}
    return decision


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
