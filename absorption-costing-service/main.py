"""
Vimbai Absorption Costing Service
Manages total costing / absorption costing methods.
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

SERVICE_NAME = "absorption-costing-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8064"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Absorption Costing Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class CostComponent(BaseModel):
    component_name: str
    amount: float
    cost_type: str  # direct_material, direct_labor, direct_expense, manufacturing_overhead
    absorbed: bool = True


class ProductCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    product_name: str
    period: str
    direct_materials: float = 0
    direct_labor: float = 0
    direct_expenses: float = 0
    prime_cost: float = 0
    manufacturing_overhead: float = 0
    total_production_cost: float = 0
    units_produced: int = 0
    cost_per_unit: float = 0
    opening_stock: int = 0
    closing_stock: int = 0
    cost_components: List[CostComponent] = []
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class OverheadAbsorption(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    period: str
    overhead_cost: float
    absorption_base: str  # machine_hours, labor_hours, units, etc.
    absorption_base_units: float
    overhead_absorption_rate: float = 0
    absorbed_overhead: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


product_costs: List[ProductCost] = []
overhead_absorptions: List[OverheadAbsorption] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Absorption costing management"}


@app.post("/product-costs/calculate")
async def calculate_product_cost(
    product_id: str, product_name: str, period: str,
    direct_materials: float, direct_labor: float, direct_expenses: float,
    manufacturing_overhead: float, units_produced: int,
    opening_stock: int = 0, closing_stock: int = 0,
    cost_components: Optional[List[Dict[str, Any]]] = None
):
    """Calculate full product cost using absorption costing."""
    product_cost = ProductCost(
        product_id=product_id, product_name=product_name, period=period,
        direct_materials=direct_materials, direct_labor=direct_labor,
        direct_expenses=direct_expenses, manufacturing_overhead=manufacturing_overhead,
        units_produced=units_produced, opening_stock=opening_stock, closing_stock=closing_stock
    )

    # Calculate prime cost
    product_cost.prime_cost = direct_materials + direct_labor + direct_expenses

    # Calculate total production cost
    product_cost.total_production_cost = product_cost.prime_cost + manufacturing_overhead

    # Calculate cost per unit
    if units_produced > 0:
        product_cost.cost_per_unit = product_cost.total_production_cost / units_produced

    # Add cost components if provided
    if cost_components:
        for comp in cost_components:
            product_cost.cost_components.append(CostComponent(**comp))

    # Create journal entry
    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Product cost calculation - {product_name} ({period})",
        "entries": [
            {"account_code": "1500", "description": "Work in Progress", "debit": product_cost.total_production_cost, "credit": 0},
            {"account_code": "1100", "description": "Raw Materials", "debit": 0, "credit": direct_materials},
            {"account_code": "2100", "description": "Direct Labor", "debit": 0, "credit": direct_labor},
            {"account_code": "2200", "description": "Manufacturing Overhead", "debit": 0, "credit": manufacturing_overhead},
        ],
        "reference": f"ABS-COST-{product_cost.id[:8]}"
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    product_cost.journal_entry_id = result.get("id")
    product_costs.append(product_cost)

    return product_cost


@app.post("/overhead/absorption")
async def calculate_overhead_absorption(
    product_id: str, period: str, overhead_cost: float,
    absorption_base: str, absorption_base_units: float
):
    """Calculate overhead absorption rate and absorbed overhead."""
    absorption = OverheadAbsorption(
        product_id=product_id, period=period, overhead_cost=overhead_cost,
        absorption_base=absorption_base, absorption_base_units=absorption_base_units
    )

    # Calculate overhead absorption rate
    if absorption_base_units > 0:
        absorption.overhead_absorption_rate = overhead_cost / absorption_base_units
        absorption.absorbed_overhead = absorption.overhead_absorption_rate * absorption_base_units

    overhead_absorptions.append(absorption)
    return absorption


@app.post("/cost-plus")
async def calculate_cost_plus_pricing(
    product_cost: float, markup_percentage: float
):
    """Calculate selling price using cost-plus pricing."""
    markup_amount = product_cost * (markup_percentage / 100)
    selling_price = product_cost + markup_amount

    return {
        "product_cost": product_cost,
        "markup_percentage": markup_percentage,
        "markup_amount": markup_amount,
        "selling_price": selling_price
    }


@app.get("/product-costs")
async def list_product_costs(
    product_id: Optional[str] = None,
    period: Optional[str] = None
):
    """List product costs."""
    result = product_costs
    if product_id:
        result = [p for p in result if p.product_id == product_id]
    if period:
        result = [p for p in result if p.period == period]
    return {"product_costs": result}


@app.get("/product-costs/{product_id}/latest")
async def get_latest_product_cost(product_id: str):
    """Get latest product cost."""
    product_cost = next((p for p in reversed(product_costs) if p.product_id == product_id), None)
    if not product_cost:
        return {"error": "Product cost not found"}
    return product_cost


@app.get("/stock-valuation")
async def calculate_stock_valuation(product_id: str, valuation_method: str = "fifo"):
    """Calculate stock valuation using absorption costing."""
    product_cost = next((p for p in reversed(product_costs) if p.product_id == product_id), None)
    if not product_cost:
        return {"error": "Product cost not found"}

    closing_stock_value = product_cost.cost_per_unit * product_cost.closing_stock

    return {
        "product_id": product_id,
        "valuation_method": valuation_method,
        "cost_per_unit": product_cost.cost_per_unit,
        "closing_stock_units": product_cost.closing_stock,
        "closing_stock_value": closing_stock_value
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)