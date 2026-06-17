"""
FinAcc Break-Even Output Service
Calculates break-even output in units.
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

SERVICE_NAME = "break-even-output-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8081"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Break-Even Output Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class BreakEvenOutput(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_id: str
    entity_name: str
    period: str
    fixed_costs: float
    selling_price_per_unit: float
    variable_cost_per_unit: float
    contribution_per_unit: float = 0
    break_even_output: float = 0
    target_output: float = 0
    target_profit: float = 0
    formula: str = ""
    created_at: datetime = Field(default_factory=datetime.utcnow)


outputs: List[BreakEvenOutput] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Break-even output calculation"}


@app.post("/calculate")
async def calculate_break_even_output(
    entity_id: str, entity_name: str, period: str,
    fixed_costs: float, selling_price_per_unit: float,
    variable_cost_per_unit: float, target_profit: float = 0
):
    """Calculate break-even output in units."""
    output = BreakEvenOutput(
        entity_id=entity_id, entity_name=entity_name, period=period,
        fixed_costs=fixed_costs, selling_price_per_unit=selling_price_per_unit,
        variable_cost_per_unit=variable_cost_per_unit, target_profit=target_profit
    )

    # Calculate contribution per unit
    output.contribution_per_unit = selling_price_per_unit - variable_cost_per_unit

    # Calculate break-even output
    # Formula: Fixed Costs / Contribution per unit
    if output.contribution_per_unit > 0:
        output.break_even_output = fixed_costs / output.contribution_per_unit
        output.formula = f"{fixed_costs} / {output.contribution_per_unit} = {output.break_even_output}"

    # Calculate target output for profit
    if target_profit > 0 and output.contribution_per_unit > 0:
        output.target_output = (fixed_costs + target_profit) / output.contribution_per_unit

    outputs.append(output)
    return output


@app.post("/multi-product")
async def calculate_multi_product_output(
    entity_name: str, fixed_costs: float,
    products: List[Dict[str, Any]]  # [{product_name, selling_price, variable_cost, sales_mix_percentage}]
):
    """Calculate break-even output for multiple products."""
    results = []

    # Calculate weighted average contribution
    weighted_contribution = 0
    total_units_needed = 0

    for prod in products:
        price = prod["selling_price"]
        var_cost = prod["variable_cost"]
        mix = prod.get("sales_mix_percentage", 0)
        contribution = price - var_cost
        weighted_contribution += contribution * (mix / 100)

    # Calculate total break-even units
    if weighted_contribution > 0:
        total_break_even_units = fixed_costs / weighted_contribution
    else:
        total_break_even_units = float('inf')

    # Calculate per product
    for prod in products:
        mix = prod.get("sales_mix_percentage", 0)
        units = total_break_even_units * (mix / 100)
        contribution = prod["selling_price"] - prod["variable_cost"]

        results.append({
            "product_name": prod["product_name"],
            "break_even_units": units,
            "selling_price": prod["selling_price"],
            "variable_cost": prod["variable_cost"],
            "contribution_per_unit": contribution,
            "sales_mix": mix
        })

    return {
        "entity_name": entity_name,
        "fixed_costs": fixed_costs,
        "weighted_average_contribution": weighted_contribution,
        "total_break_even_units": total_break_even_units,
        "breakdown": results
    }


@app.get("/outputs")
async def list_outputs(entity_id: Optional[str] = None):
    """List break-even outputs."""
    result = outputs
    if entity_id:
        result = [o for o in result if o.entity_id == entity_id]
    return {"outputs": result}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)