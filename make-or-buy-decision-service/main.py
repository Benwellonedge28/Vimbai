"""
FinAcc Make or Buy Decision Service
Helps decide between making internally or buying externally.
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

SERVICE_NAME = "make-or-buy-decision-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8073"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Make or Buy Decision Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class MakeCost(BaseModel):
    direct_materials: float
    direct_labor: float
    variable_overhead: float
    fixed_overhead: float = 0
    total_make_cost: float = 0


class BuyCost(BaseModel):
    purchase_price: float
    delivery_cost: float = 0
    inspection_cost: float = 0
    additional_overhead: float = 0
    total_buy_cost: float = 0


class MakeOrBuyDecision(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    component_id: str
    component_name: str
    annual_demand: float
    make_cost: MakeCost
    buy_cost: BuyCost
    avoidable_fixed_costs: float = 0  # Fixed costs that can be avoided if buying
    unavoidable_costs: float = 0  # Costs that continue even if outsourcing
    contribution_lost: float = 0  # If buying means losing contribution
    make_or_buy_recommendation: str = ""
    financial_advantage: float = 0
    qualitative_factors: List[str] = []
    created_at: datetime = Field(default_factory=datetime.utcnow)


make_or_buy_decisions: List[MakeOrBuyDecision] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Make or buy decision analysis"}


@app.post("/analyze")
async def analyze_make_or_buy(
    component_id: str, component_name: str, annual_demand: float,
    # Make costs
    direct_materials: float, direct_labor: float,
    variable_overhead: float, fixed_overhead: float = 0,
    # Buy costs
    purchase_price: float, delivery_cost: float = 0,
    inspection_cost: float = 0, additional_overhead: float = 0,
    avoidable_fixed_costs: float = 0, unavoidable_costs: float = 0,
    contribution_lost: float = 0,
    qualitative_factors: Optional[List[str]] = None
):
    """Analyze make or buy decision."""
    make_cost = MakeCost(
        direct_materials=direct_materials, direct_labor=direct_labor,
        variable_overhead=variable_overhead, fixed_overhead=fixed_overhead
    )
    make_cost.total_make_cost = direct_materials + direct_labor + variable_overhead + fixed_overhead

    buy_cost = BuyCost(
        purchase_price=purchase_price, delivery_cost=delivery_cost,
        inspection_cost=inspection_cost, additional_overhead=additional_overhead
    )
    buy_cost.total_buy_cost = purchase_price + delivery_cost + inspection_cost + additional_overhead

    # Calculate relevant costs for decision
    relevant_make_cost = make_cost.total_make_cost - avoidable_fixed_costs
    relevant_buy_cost = buy_cost.total_buy_cost + contribution_lost + unavoidable_costs

    financial_advantage = relevant_make_cost - relevant_buy_cost

    if financial_advantage > 0:
        recommendation = "MAKE"
    elif financial_advantage < 0:
        recommendation = "BUY"
    else:
        recommendation = "INDIFFERENT"

    decision = MakeOrBuyDecision(
        component_id=component_id, component_name=component_name,
        annual_demand=annual_demand, make_cost=make_cost, buy_cost=buy_cost,
        avoidable_fixed_costs=avoidable_fixed_costs, unavoidable_costs=unavoidable_costs,
        contribution_lost=contribution_lost,
        make_or_buy_recommendation=recommendation,
        financial_advantage=financial_advantage,
        qualitative_factors=qualitative_factors or []
    )

    make_or_buy_decisions.append(decision)
    return decision


@app.post("/analyze-per-unit")
async def analyze_per_unit(
    component_name: str,
    make_cost_per_unit: float, buy_cost_per_unit: float,
    avoidable_cost_per_unit: float = 0
):
    """Quick per-unit make or buy analysis."""
    relevant_make_cost = make_cost_per_unit
    relevant_buy_cost = buy_cost_per_unit + avoidable_cost_per_unit

    advantage = relevant_make_cost - relevant_buy_cost

    if advantage > 0:
        recommendation = "MAKE"
        reason = f"Make costs {abs(advantage)} less per unit"
    elif advantage < 0:
        recommendation = "BUY"
        reason = f"Buy costs {abs(advantage)} less per unit"
    else:
        recommendation = "INDIFFERENT"
        reason = "Costs are equal"

    return {
        "component_name": component_name,
        "make_cost_per_unit": make_cost_per_unit,
        "buy_cost_per_unit": buy_cost_per_unit,
        "avoidable_cost_per_unit": avoidable_cost_per_unit,
        "relevant_make_cost": relevant_make_cost,
        "relevant_buy_cost": relevant_buy_cost,
        "financial_advantage": advantage,
        "recommendation": recommendation,
        "reason": reason
    }


@app.get("/decisions")
async def list_decisions(component_id: Optional[str] = None):
    """List make or buy decisions."""
    result = make_or_buy_decisions
    if component_id:
        result = [d for d in result if d.component_id == component_id]
    return {"decisions": result}


@app.get("/decisions/{decision_id}")
async def get_decision(decision_id: str):
    """Get decision details."""
    decision = next((d for d in make_or_buy_decisions if d.id == decision_id), None)
    if not decision:
        return {"error": "Decision not found"}
    return decision


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)