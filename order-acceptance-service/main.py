"""
Vimbai Order Acceptance Below Selling Price Service
Analyzes whether to accept orders below normal selling price.
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

SERVICE_NAME = "order-acceptance-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8077"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Order Acceptance Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class OrderAcceptanceDecision(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    order_id: str
    product_id: str
    product_name: str
    customer_name: str
    normal_selling_price: float
    offered_price: float
    price_below_normal: float = 0
    price_below_normal_percentage: float = 0
    variable_cost_per_unit: float
    full_cost_per_unit: float = 0
    contribution_per_unit_at_normal: float = 0
    contribution_per_unit_at_offered: float = 0
    contribution_from_order: float = 0
    total_fixed_cost_incremental: float = 0
    units_requested: float
    total_relevant_cost: float = 0
    minimum_acceptable_price: float = 0
    opportunity_cost: float = 0  # Contribution lost from displaced regular sales
    net_contribution: float = 0
    recommendation: str = ""
    conditions: List[str] = []
    created_at: datetime = Field(default_factory=datetime.utcnow)


decisions: List[OrderAcceptanceDecision] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Order acceptance analysis"}


@app.post("/analyze")
async def analyze_order_acceptance(
    order_id: str, product_id: str, product_name: str, customer_name: str,
    normal_selling_price: float, offered_price: float,
    variable_cost_per_unit: float, units_requested: float,
    full_cost_per_unit: Optional[float] = None,
    incremental_fixed_costs: float = 0,
    displaced_sales_units: float = 0,
    displaced_contribution_per_unit: float = 0
):
    """Analyze order acceptance decision."""
    decision = OrderAcceptanceDecision(
        order_id=order_id, product_id=product_id, product_name=product_name,
        customer_name=customer_name, normal_selling_price=normal_selling_price,
        offered_price=offered_price, variable_cost_per_unit=variable_cost_per_unit,
        units_requested=units_requested,
        total_fixed_cost_incremental=incremental_fixed_costs
    )

    # Calculate price difference
    decision.price_below_normal = normal_selling_price - offered_price
    decision.price_below_normal_percentage = (
        decision.price_below_normal / normal_selling_price * 100 if normal_selling_price > 0 else 0
    )

    # Calculate contributions
    decision.contribution_per_unit_at_normal = normal_selling_price - variable_cost_per_unit
    decision.contribution_per_unit_at_offered = offered_price - variable_cost_per_unit
    decision.contribution_from_order = decision.contribution_per_unit_at_offered * units_requested

    # Calculate total relevant cost
    variable_cost_total = variable_cost_per_unit * units_requested
    decision.total_relevant_cost = variable_cost_total + incremental_fixed_costs

    # Calculate minimum acceptable price
    if units_requested > 0:
        # Price that covers variable cost + incremental fixed costs
        decision.minimum_acceptable_price = (
            variable_cost_per_unit + (incremental_fixed_costs / units_requested)
        )

    # Calculate opportunity cost (contribution lost from displaced sales)
    decision.opportunity_cost = displaced_sales_units * displaced_contribution_per_unit

    # Calculate net contribution
    decision.net_contribution = decision.contribution_from_order - decision.opportunity_cost

    # Full cost if provided
    if full_cost_per_unit:
        decision.full_cost_per_unit = full_cost_per_unit

    # Make recommendation
    if decision.opportunity_cost == 0:
        if decision.contribution_per_unit_at_offered > 0:
            decision.recommendation = "ACCEPT"
            decision.conditions = ["Positive contribution generated"]
        else:
            decision.recommendation = "REJECT"
            decision.conditions = ["Negative contribution - would lose money"]
    else:
        if decision.net_contribution > 0:
            decision.recommendation = "ACCEPT_WITH_CONDITIONS"
            decision.conditions = [
                f"Net contribution: {decision.net_contribution}",
                f"Consider impact on existing customers"
            ]
        elif decision.net_contribution == 0:
            decision.recommendation = "INDIFFERENT"
            decision.conditions = [
                "Consider strategic value of customer relationship"
            ]
        else:
            decision.recommendation = "REJECT"
            decision.conditions = [
                f"Net contribution negative: {decision.net_contribution}",
                "Would reduce overall profitability"
            ]

    decisions.append(decision)
    return decision


@app.post("/one-time-order")
async def analyze_one_time_order(
    product_name: str, offered_price: float,
    variable_cost_per_unit: float, units: float,
    has_spare_capacity: bool = True
):
    """Quick analysis for one-time special order."""
    contribution_per_unit = offered_price - variable_cost_per_unit
    total_contribution = contribution_per_unit * units

    minimum_price = variable_cost_per_unit

    if has_spare_capacity:
        if contribution_per_unit > 0:
            recommendation = "ACCEPT"
            reason = f"Spare capacity available. Generates {total_contribution} contribution."
        else:
            recommendation = "REJECT"
            reason = "Price below variable cost."
    else:
        if contribution_per_unit > 0:
            recommendation = "CONSIDER"
            reason = "No spare capacity - would need to reduce regular production"
        else:
            recommendation = "REJECT"
            reason = "Price below variable cost"

    return {
        "product_name": product_name,
        "offered_price": offered_price,
        "variable_cost_per_unit": variable_cost_per_unit,
        "units": units,
        "contribution_per_unit": contribution_per_unit,
        "total_contribution": total_contribution,
        "minimum_acceptable_price": minimum_price,
        "has_spare_capacity": has_spare_capacity,
        "recommendation": recommendation,
        "reason": reason
    }


@app.get("/decisions")
async def list_decisions(
    order_id: Optional[str] = None,
    product_id: Optional[str] = None,
    recommendation: Optional[str] = None
):
    """List order acceptance decisions."""
    result = decisions
    if order_id:
        result = [d for d in result if d.order_id == order_id]
    if product_id:
        result = [d for d in result if d.product_id == product_id]
    if recommendation:
        result = [d for d in result if d.recommendation == recommendation]
    return {"decisions": result}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)