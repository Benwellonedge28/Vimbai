"""
FinAcc Break-Even Point Service
Calculates break-even point in units and revenue.
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

SERVICE_NAME = "break-even-point-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8079"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Break-Even Point Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class BreakEvenPoint(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_id: str
    entity_name: str
    period: str
    fixed_costs: float
    selling_price_per_unit: float
    variable_cost_per_unit: float
    contribution_per_unit: float = 0
    contribution_margin_ratio: float = 0
    break_even_units: float = 0
    break_even_revenue: float = 0
    formula_used: str = ""
    created_at: datetime = Field(default_factory=datetime.utcnow)


break_even_points: List[BreakEvenPoint] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Break-even point calculation"}


@app.post("/calculate")
async def calculate_break_even_point(
    entity_id: str, entity_name: str, period: str,
    fixed_costs: float, selling_price_per_unit: float, variable_cost_per_unit: float
):
    """Calculate break-even point."""
    bep = BreakEvenPoint(
        entity_id=entity_id, entity_name=entity_name, period=period,
        fixed_costs=fixed_costs, selling_price_per_unit=selling_price_per_unit,
        variable_cost_per_unit=variable_cost_per_unit
    )

    # Calculate contribution per unit
    bep.contribution_per_unit = selling_price_per_unit - variable_cost_per_unit

    # Calculate contribution margin ratio
    if selling_price_per_unit > 0:
        bep.contribution_margin_ratio = bep.contribution_per_unit / selling_price_per_unit

    # Calculate break-even point in units
    # Formula: Fixed Costs / Contribution per unit
    if bep.contribution_per_unit > 0:
        bep.break_even_units = fixed_costs / bep.contribution_per_unit
        bep.formula_used = f"{fixed_costs} / {bep.contribution_per_unit} = {bep.break_even_units}"

    # Calculate break-even revenue
    bep.break_even_revenue = bep.break_even_units * selling_price_per_unit

    break_even_points.append(bep)
    return bep


@app.post("/calculate-from-ratio")
async def calculate_from_contribution_ratio(
    entity_name: str, fixed_costs: float,
    contribution_margin_ratio: float, selling_price: float
):
    """Calculate BEP using contribution margin ratio."""
    # BEP Revenue = Fixed Costs / Contribution Margin Ratio
    if contribution_margin_ratio > 0:
        break_even_revenue = fixed_costs / contribution_margin_ratio
        break_even_units = break_even_revenue / selling_price
    else:
        break_even_revenue = float('inf')
        break_even_units = float('inf')

    return {
        "entity_name": entity_name,
        "fixed_costs": fixed_costs,
        "contribution_margin_ratio": contribution_margin_ratio,
        "selling_price": selling_price,
        "break_even_units": break_even_units,
        "break_even_revenue": break_even_revenue,
        "formula": f"{fixed_costs} / {contribution_margin_ratio} = {break_even_revenue}"
    }


@app.post("/target-profit-units")
async def calculate_units_for_target_profit(
    fixed_costs: float, contribution_per_unit: float, target_profit: float
):
    """Calculate units needed for target profit."""
    # Formula: (Fixed Costs + Target Profit) / Contribution per unit
    units_required = (fixed_costs + target_profit) / contribution_per_unit if contribution_per_unit > 0 else 0

    return {
        "fixed_costs": fixed_costs,
        "contribution_per_unit": contribution_per_unit,
        "target_profit": target_profit,
        "units_required": units_required,
        "formula": f"({fixed_costs} + {target_profit}) / {contribution_per_unit} = {units_required}"
    }


@app.get("/break-even-points")
async def list_break_even_points(entity_id: Optional[str] = None):
    """List break-even points."""
    result = break_even_points
    if entity_id:
        result = [b for b in result if b.entity_id == entity_id]
    return {"break_even_points": result}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)