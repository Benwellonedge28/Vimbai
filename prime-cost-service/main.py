"""
FinAcc Prime Cost Service
Calculates prime cost (direct materials + direct labor + direct expenses).
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

SERVICE_NAME = "prime-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8067"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Prime Cost Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class DirectCostItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    item_name: str
    item_type: str  # direct_material, direct_labor, direct_expense
    amount: float
    units: float = 1
    cost_per_unit: float = 0
    product_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class PrimeCostCalculation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    period: str
    direct_materials: float = 0
    direct_labor: float = 0
    direct_expenses: float = 0
    prime_cost: float = 0
    cost_breakdown: Dict[str, float] = {}
    created_at: datetime = Field(default_factory=datetime.utcnow)


direct_cost_items: List[DirectCostItem] = []
prime_cost_calculations: List[PrimeCostCalculation] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Prime cost calculation"}


@app.post("/direct-costs/add")
async def add_direct_cost_item(
    item_name: str, item_type: str, amount: float,
    units: float = 1, product_id: Optional[str] = None
):
    """Add a direct cost item."""
    item = DirectCostItem(
        item_name=item_name, item_type=item_type, amount=amount,
        units=units, product_id=product_id
    )
    item.cost_per_unit = amount / units if units > 0 else 0
    direct_cost_items.append(item)
    return item


@app.post("/calculate")
async def calculate_prime_cost(
    product_id: str, period: str,
    direct_materials: float, direct_labor: float,
    direct_expenses: float = 0
):
    """Calculate prime cost for a product."""
    calculation = PrimeCostCalculation(
        product_id=product_id, period=period,
        direct_materials=direct_materials,
        direct_labor=direct_labor,
        direct_expenses=direct_expenses
    )

    # Calculate Prime Cost
    calculation.prime_cost = direct_materials + direct_labor + direct_expenses

    # Build breakdown
    calculation.cost_breakdown = {
        "direct_materials": direct_materials,
        "direct_labor": direct_labor,
        "direct_expenses": direct_expenses,
        "prime_cost": calculation.prime_cost
    }

    prime_cost_calculations.append(calculation)
    return calculation


@app.post("/calculate-with-items")
async def calculate_prime_cost_from_items(
    product_id: str, period: str
):
    """Calculate prime cost from direct cost items."""
    items = [i for i in direct_cost_items if i.product_id == product_id]

    direct_materials = sum(i.amount for i in items if i.item_type == "direct_material")
    direct_labor = sum(i.amount for i in items if i.item_type == "direct_labor")
    direct_expenses = sum(i.amount for i in items if i.item_type == "direct_expense")

    calculation = PrimeCostCalculation(
        product_id=product_id, period=period,
        direct_materials=direct_materials,
        direct_labor=direct_labor,
        direct_expenses=direct_expenses
    )
    calculation.prime_cost = direct_materials + direct_labor + direct_expenses
    calculation.cost_breakdown = {
        "direct_materials": direct_materials,
        "direct_labor": direct_labor,
        "direct_expenses": direct_expenses,
        "prime_cost": calculation.prime_cost
    }

    prime_cost_calculations.append(calculation)
    return calculation


@app.get("/calculations")
async def list_calculations(product_id: Optional[str] = None):
    """List prime cost calculations."""
    result = prime_cost_calculations
    if product_id:
        result = [c for c in result if c.product_id == product_id]
    return {"calculations": result}


@app.get("/direct-costs")
async def list_direct_cost_items(product_id: Optional[str] = None):
    """List direct cost items."""
    result = direct_cost_items
    if product_id:
        result = [i for i in result if i.product_id == product_id]
    return {"items": result}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)