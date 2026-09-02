"""
Vimbai Overhead Absorption Rate Service
Calculates overhead absorption rates for cost centres.
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

SERVICE_NAME = "overhead-absorption-rate-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8090"))
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

app = FastAPI(title="Vimbai Overhead Absorption Rate Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class OverheadAbsorptionRate(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_centre_name: str
    cost_centre_id: str
    overhead_type: str  # production, service, administrative
    absorption_base: str  # machine_hours, direct_labour_hours, direct_labour_cost, material_cost, etc.
    budgeted_overhead: float
    budgeted_base_units: float
    absorption_rate: float = 0
    rate_per_unit: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


class OARAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    entity_name: str
    cost_centre_name: str
    rate_type: str  # blanket, departmental
    absorption_base: str
    calculated_rate: float
    base_units_used: float
    overhead_absorbed: float
    created_at: datetime = Field(default_factory=datetime.utcnow)


absorption_rates: List[OverheadAbsorptionRate] = []
oar_analyses: List[OARAnalysis] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Overhead absorption rate calculation"}


@app.post("/calculate")
async def calculate_oar(
    cost_centre_name: str,
    cost_centre_id: str,
    budgeted_overhead: float,
    budgeted_base_units: float,
    overhead_type: str = "production",
    absorption_base: str = "machine_hours",
):
    """Calculate overhead absorption rate."""
    rate = OverheadAbsorptionRate(
        cost_centre_name=cost_centre_name,
        cost_centre_id=cost_centre_id,
        budgeted_overhead=budgeted_overhead,
        budgeted_base_units=budgeted_base_units,
        overhead_type=overhead_type,
        absorption_base=absorption_base,
    )

    # Calculate absorption rate
    if budgeted_base_units > 0:
        rate.absorption_rate = budgeted_overhead / budgeted_base_units
        rate.rate_per_unit = rate.absorption_rate

    absorption_rates.append(rate)
    return rate


@app.post("/machine-hours-rate")
async def calculate_machine_hours_rate(cost_centre_name: str, budgeted_overhead: float, budgeted_machine_hours: float):
    """Calculate OAR based on machine hours."""
    rate_per_mh = budgeted_overhead / budgeted_machine_hours if budgeted_machine_hours > 0 else 0

    analysis = OARAnalysis(
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="machine_hours",
        calculated_rate=rate_per_mh,
        base_units_used=budgeted_machine_hours,
        overhead_absorbed=budgeted_overhead,
    )
    oar_analyses.append(analysis)

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "machine_hours",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_machine_hours": budgeted_machine_hours,
        "oar_per_machine_hour": rate_per_mh,
        "formula": f"{budgeted_overhead} / {budgeted_machine_hours} = {rate_per_mh}",
    }


@app.post("/labour-hours-rate")
async def calculate_labour_hours_rate(
    cost_centre_name: str, budgeted_overhead: float, budgeted_direct_labour_hours: float
):
    """Calculate OAR based on direct labour hours."""
    rate_per_dlh = budgeted_overhead / budgeted_direct_labour_hours if budgeted_direct_labour_hours > 0 else 0

    analysis = OARAnalysis(
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="direct_labour_hours",
        calculated_rate=rate_per_dlh,
        base_units_used=budgeted_direct_labour_hours,
        overhead_absorbed=budgeted_overhead,
    )
    oar_analyses.append(analysis)

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "direct_labour_hours",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_direct_labour_hours": budgeted_direct_labour_hours,
        "oar_per_direct_labour_hour": rate_per_dlh,
    }


@app.post("/labour-cost-rate")
async def calculate_labour_cost_rate(
    cost_centre_name: str, budgeted_overhead: float, budgeted_direct_labour_cost: float
):
    """Calculate OAR based on direct labour cost (percentage)."""
    rate_percentage = (budgeted_overhead / budgeted_direct_labour_cost * 100) if budgeted_direct_labour_cost > 0 else 0

    analysis = OARAnalysis(
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="direct_labour_cost",
        calculated_rate=rate_percentage,
        base_units_used=budgeted_direct_labour_cost,
        overhead_absorbed=budgeted_overhead,
    )
    oar_analyses.append(analysis)

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "direct_labour_cost",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_direct_labour_cost": budgeted_direct_labour_cost,
        "oar_percentage": rate_percentage,
        "formula": f"({budgeted_overhead} / {budgeted_direct_labour_cost}) × 100 = {rate_percentage}%",
    }


@app.post("/material-cost-rate")
async def calculate_material_cost_rate(cost_centre_name: str, budgeted_overhead: float, budgeted_material_cost: float):
    """Calculate OAR based on material cost (percentage)."""
    rate_percentage = (budgeted_overhead / budgeted_material_cost * 100) if budgeted_material_cost > 0 else 0

    analysis = OARAnalysis(
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="material_cost",
        calculated_rate=rate_percentage,
        base_units_used=budgeted_material_cost,
        overhead_absorbed=budgeted_overhead,
    )
    oar_analyses.append(analysis)

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "material_cost",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_material_cost": budgeted_material_cost,
        "oar_percentage": rate_percentage,
    }


@app.post("/prime-cost-rate")
async def calculate_prime_cost_rate(cost_centre_name: str, budgeted_overhead: float, budgeted_prime_cost: float):
    """Calculate OAR based on prime cost (percentage)."""
    rate_percentage = (budgeted_overhead / budgeted_prime_cost * 100) if budgeted_prime_cost > 0 else 0

    analysis = OARAnalysis(
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="prime_cost",
        calculated_rate=rate_percentage,
        base_units_used=budgeted_prime_cost,
        overhead_absorbed=budgeted_overhead,
    )
    oar_analyses.append(analysis)

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "prime_cost",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_prime_cost": budgeted_prime_cost,
        "oar_percentage": rate_percentage,
    }


@app.post("/unit-rate")
async def calculate_unit_rate(cost_centre_name: str, budgeted_overhead: float, budgeted_units: float):
    """Calculate OAR based on units produced."""
    rate_per_unit = budgeted_overhead / budgeted_units if budgeted_units > 0 else 0

    analysis = OARAnalysis(
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="units",
        calculated_rate=rate_per_unit,
        base_units_used=budgeted_units,
        overhead_absorbed=budgeted_overhead,
    )
    oar_analyses.append(analysis)

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "units",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_units": budgeted_units,
        "oar_per_unit": rate_per_unit,
    }


@app.post("/blanket-rate")
async def calculate_blanket_rate(
    entity_name: str, total_budgeted_overhead: float, total_base_units: float, absorption_base: str
):
    """Calculate blanket OAR for entire factory."""
    blanket_rate = total_budgeted_overhead / total_base_units if total_base_units > 0 else 0

    analysis = OARAnalysis(
        entity_name=entity_name,
        cost_centre_name="entire_factory",
        rate_type="blanket",
        absorption_base=absorption_base,
        calculated_rate=blanket_rate,
        base_units_used=total_base_units,
        overhead_absorbed=total_budgeted_overhead,
    )
    oar_analyses.append(analysis)

    return {
        "entity_name": entity_name,
        "rate_type": "blanket",
        "absorption_base": absorption_base,
        "total_budgeted_overhead": total_budgeted_overhead,
        "total_base_units": total_base_units,
        "blanket_oar": blanket_rate,
    }


@app.post("/absorbed-overhead")
async def calculate_absorbed_overhead(cost_centre_name: str, actual_base_units: float, absorption_rate: float):
    """Calculate overhead absorbed based on actual activity."""
    absorbed = actual_base_units * absorption_rate
    return {
        "cost_centre_name": cost_centre_name,
        "actual_base_units": actual_base_units,
        "absorption_rate": absorption_rate,
        "overhead_absorbed": absorbed,
        "formula": f"{actual_base_units} × {absorption_rate} = {absorbed}",
    }


@app.get("/rates")
async def list_rates(cost_centre_id: Optional[str] = None):
    """List overhead absorption rates."""
    result = absorption_rates
    if cost_centre_id:
        result = [r for r in result if r.cost_centre_id == cost_centre_id]
    return {"rates": result}


@app.get("/analyses")
async def list_analyses():
    """List OAR analyses."""
    return {"analyses": oar_analyses}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
