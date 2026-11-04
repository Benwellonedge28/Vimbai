"""
Vimbai Target Costing Service
Target cost determination, cost reduction gap analysis, and value engineering.
Port: 8382
"""

import importlib.util
import os
import sys
import uuid
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "target-costing-service"
PORT = int(os.getenv("PORT", "8382"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("target_costing_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["target_costing_calc"] = calc
_spec.loader.exec_module(calc)

logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Target Costing Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class TargetCostRequest(BaseModel):
    company_id: str
    product_name: str
    target_selling_price: float
    desired_profit_margin_pct: float
    current_cost: float
    component_costs: Dict[str, float] = {}


class TargetCostResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    product_name: str
    target_selling_price: float
    desired_margin: float
    target_profit: float
    target_cost: float
    current_cost: float
    cost_reduction_needed: float
    cost_reduction_pct: float
    feasible: bool
    component_analysis: List[Dict] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


def _reference(payload):
    """The original Python calculation, kept as the oracle."""
    target_profit = payload["target_selling_price"] * (payload["desired_profit_margin_pct"] / 100)
    target_cost = payload["target_selling_price"] - target_profit
    cost_reduction = payload["current_cost"] - target_cost
    cost_reduction_pct = (cost_reduction / payload["current_cost"] * 100) if payload["current_cost"] else 0
    feasible = cost_reduction <= payload["current_cost"] * 0.3  # 30% reduction is max feasible

    components = []
    total_component = sum(payload["component_costs"].values())
    for name, cost in payload["component_costs"].items():
        proportion = cost / total_component if total_component else 0
        reduction_needed = cost * (cost_reduction_pct / 100)
        components.append(
            {
                "component": name,
                "current_cost": round(cost, 2),
                "proportion_of_total": round(proportion * 100, 1),
                "suggested_reduction": round(reduction_needed, 2),
                "target_cost": round(cost - reduction_needed, 2),
            }
        )

    return dict(
        company_id=payload["company_id"],
        product_name=payload["product_name"],
        target_selling_price=round(payload["target_selling_price"], 2),
        desired_margin=round(payload["desired_profit_margin_pct"], 2),
        target_profit=round(target_profit, 2),
        target_cost=round(target_cost, 2),
        current_cost=round(payload["current_cost"], 2),
        cost_reduction_needed=round(cost_reduction, 2),
        cost_reduction_pct=round(cost_reduction_pct, 2),
        feasible=feasible,
        component_analysis=components,
    )


@app.post("/calculate", response_model=TargetCostResult)
async def calculate_target_cost(request: TargetCostRequest):
    logger.info("Calculating target cost", product=request.product_name)
    result = calc.calculate(request.model_dump(), _reference)
    result.setdefault("id", str(uuid.uuid4()))
    return TargetCostResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
