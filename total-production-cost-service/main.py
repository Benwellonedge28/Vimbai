"""
Vimbai Total Production Cost Service
Calculates total cost of production (Prime Cost + Factory Overhead).
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

SERVICE_NAME = "total-production-cost-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8068"))
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

app = FastAPI(title="Vimbai Total Production Cost Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class ProductionCostCalculation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    product_id: str
    department_id: str
    period: str

    # Prime Cost Components
    direct_materials: float = 0
    direct_labor: float = 0
    direct_expenses: float = 0
    prime_cost: float = 0

    # Factory Overhead
    factory_rent: float = 0
    factory_depreciation: float = 0
    factory_insurance: float = 0
    factory_maintenance: float = 0
    other_overhead: float = 0
    total_overhead: float = 0

    # Work in Progress
    wip_opening: float = 0
    wip_closing: float = 0

    # Total Production Cost
    total_production_cost: float = 0
    units_produced: int = 0
    cost_per_unit: float = 0

    created_at: datetime = Field(default_factory=datetime.utcnow)


production_costs: List[ProductionCostCalculation] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Total production cost calculation"}


@app.post("/calculate")
async def calculate_total_production_cost(
    product_id: str,
    department_id: str,
    period: str,
    direct_materials: float,
    direct_labor: float,
    direct_expenses: float = 0,
    factory_rent: float = 0,
    factory_depreciation: float = 0,
    factory_insurance: float = 0,
    factory_maintenance: float = 0,
    other_overhead: float = 0,
    wip_opening: float = 0,
    wip_closing: float = 0,
    units_produced: int = 0,
):
    """Calculate total production cost."""
    calc = ProductionCostCalculation(
        product_id=product_id,
        department_id=department_id,
        period=period,
        direct_materials=direct_materials,
        direct_labor=direct_labor,
        direct_expenses=direct_expenses,
        factory_rent=factory_rent,
        factory_depreciation=factory_depreciation,
        factory_insurance=factory_insurance,
        factory_maintenance=factory_maintenance,
        other_overhead=other_overhead,
        wip_opening=wip_opening,
        wip_closing=wip_closing,
        units_produced=units_produced,
    )

    # Calculate Prime Cost
    calc.prime_cost = direct_materials + direct_labor + direct_expenses

    # Calculate Total Overhead
    calc.total_overhead = factory_rent + factory_depreciation + factory_insurance + factory_maintenance + other_overhead

    # Calculate Total Production Cost
    calc.total_production_cost = calc.prime_cost + calc.total_overhead + wip_opening - wip_closing

    # Calculate Cost Per Unit
    if units_produced > 0:
        calc.cost_per_unit = calc.total_production_cost / units_produced

    production_costs.append(calc)
    return calc


@app.get("/calculations")
async def list_calculations(
    product_id: Optional[str] = None, department_id: Optional[str] = None, period: Optional[str] = None
):
    """List production cost calculations."""
    result = production_costs
    if product_id:
        result = [c for c in result if c.product_id == product_id]
    if department_id:
        result = [c for c in result if c.department_id == department_id]
    if period:
        result = [c for c in result if c.period == period]
    return {"calculations": result}


@app.get("/calculations/{product_id}/latest")
async def get_latest_calculation(product_id: str):
    """Get latest production cost calculation."""
    calc = next((c for c in reversed(production_costs) if c.product_id == product_id), None)
    if not calc:
        return {"error": "Calculation not found"}
    return calc


@app.get("/summary")
async def get_production_cost_summary(department_id: Optional[str] = None, period: Optional[str] = None):
    """Get summary of production costs."""
    calcs = production_costs
    if department_id:
        calcs = [c for c in calcs if c.department_id == department_id]
    if period:
        calcs = [c for c in calcs if c.period == period]

    return {
        "total_prime_cost": sum(c.prime_cost for c in calcs),
        "total_overhead": sum(c.total_overhead for c in calcs),
        "total_production_cost": sum(c.total_production_cost for c in calcs),
        "total_units_produced": sum(c.units_produced for c in calcs),
        "average_cost_per_unit": (
            sum(c.total_production_cost for c in calcs) / sum(c.units_produced for c in calcs)
            if sum(c.units_produced for c in calcs) > 0
            else 0
        ),
        "calculations_count": len(calcs),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
