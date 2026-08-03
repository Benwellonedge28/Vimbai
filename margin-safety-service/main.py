"""
Vimbai Margin of Safety Service
Calculates margin of safety for products and businesses.
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

SERVICE_NAME = "margin-safety-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8082"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Margin of Safety Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class MarginOfSafety(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_id: str
    entity_name: str
    period: str
    current_output: float
    current_revenue: float
    break_even_output: float
    break_even_revenue: float
    margin_of_safety_units: float = 0
    margin_of_safety_revenue: float = 0
    margin_of_safety_percentage: float = 0
    margin_of_safety_ratio: float = 0
    risk_level: str = ""  # low, medium, high
    created_at: datetime = Field(default_factory=datetime.utcnow)


margins: List[MarginOfSafety] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Margin of safety calculation"}


@app.post("/calculate")
async def calculate_margin_of_safety(
    entity_id: str, entity_name: str, period: str,
    current_output: float, current_revenue: float,
    break_even_output: float, break_even_revenue: float
):
    """Calculate margin of safety."""
    mos = MarginOfSafety(
        entity_id=entity_id, entity_name=entity_name, period=period,
        current_output=current_output, current_revenue=current_revenue,
        break_even_output=break_even_output, break_even_revenue=break_even_revenue
    )

    # Calculate margin of safety in units
    mos.margin_of_safety_units = current_output - break_even_output

    # Calculate margin of safety in revenue
    mos.margin_of_safety_revenue = current_revenue - break_even_revenue

    # Calculate margin of safety percentage
    if current_revenue > 0:
        mos.margin_of_safety_percentage = (mos.margin_of_safety_revenue / current_revenue) * 100

    # Calculate margin of safety ratio
    if current_output > 0:
        mos.margin_of_safety_ratio = mos.margin_of_safety_units / current_output

    # Determine risk level
    if mos.margin_of_safety_percentage >= 20:
        mos.risk_level = "low"
    elif mos.margin_of_safety_percentage >= 10:
        mos.risk_level = "medium"
    else:
        mos.risk_level = "high"

    margins.append(mos)
    return mos


@app.post("/calculate-from-data")
async def calculate_from_basic_data(
    entity_name: str, selling_price: float,
    fixed_costs: float, variable_cost_per_unit: float,
    expected_sales_units: float
):
    """Calculate margin of safety from basic data."""
    contribution = selling_price - variable_cost_per_unit
    break_even_units = fixed_costs / contribution if contribution > 0 else 0
    break_even_revenue = break_even_units * selling_price
    current_revenue = expected_sales_units * selling_price

    mos_units = expected_sales_units - break_even_units
    mos_revenue = current_revenue - break_even_revenue
    mos_percentage = (mos_revenue / current_revenue * 100) if current_revenue > 0 else 0

    if mos_percentage >= 20:
        risk_level = "low"
    elif mos_percentage >= 10:
        risk_level = "medium"
    else:
        risk_level = "high"

    return {
        "entity_name": entity_name,
        "current_sales_units": expected_sales_units,
        "current_revenue": current_revenue,
        "break_even_units": break_even_units,
        "break_even_revenue": break_even_revenue,
        "margin_of_safety_units": mos_units,
        "margin_of_safety_revenue": mos_revenue,
        "margin_of_safety_percentage": mos_percentage,
        "risk_level": risk_level
    }


@app.post("/target-mos")
async def calculate_sales_for_target_mos(
    entity_name: str, break_even_revenue: float,
    target_mos_percentage: float
):
    """Calculate sales revenue needed for target margin of safety."""
    # MOS% = (Sales - BEP) / Sales
    # MOS% = 1 - (BEP / Sales)
    # Sales = BEP / (1 - MOS%)

    if target_mos_percentage < 100:
        required_sales = break_even_revenue / (1 - target_mos_percentage / 100)
    else:
        required_sales = float('inf')

    return {
        "entity_name": entity_name,
        "break_even_revenue": break_even_revenue,
        "target_mos_percentage": target_mos_percentage,
        "required_sales_revenue": required_sales
    }


@app.get("/margins")
async def list_margins(entity_id: Optional[str] = None, risk_level: Optional[str] = None):
    """List margin of safety calculations."""
    result = margins
    if entity_id:
        result = [m for m in result if m.entity_id == entity_id]
    if risk_level:
        result = [m for m in result if m.risk_level == risk_level]
    return {"margins": result}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)