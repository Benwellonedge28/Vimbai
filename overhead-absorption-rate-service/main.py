"""
Vimbai Overhead Absorption Rate Service
Departmental and blanket absorption rates, and the workings behind them.

Rates and analyses live in Neo4j, scoped to the caller and the Book. The file may be
imported bare (bracket mounts, uvicorn main:app), so it bootstraps its own package
alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "overhead_absorption_rate_service" not in _sys.modules or not hasattr(
    _sys.modules.get("overhead_absorption_rate_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "overhead_absorption_rate_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["overhead_absorption_rate_service"] = _pkg
    _sys.modules["overhead_absorption_rate_service"].__path__ = [_HERE]

import os
from typing import List, Optional

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from overhead_absorption_rate_service import crud
from overhead_absorption_rate_service.dependencies import book_id_var, get_db_session, get_user_id

SERVICE_NAME = "overhead-absorption-rate-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8090"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


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
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate overhead absorption rate."""
    return await crud.create_rate(
        db_session,
        user_id,
        cost_centre_name=cost_centre_name,
        cost_centre_id=cost_centre_id,
        budgeted_overhead=budgeted_overhead,
        budgeted_base_units=budgeted_base_units,
        overhead_type=overhead_type,
        absorption_base=absorption_base,
    )


@app.post("/machine-hours-rate")
async def calculate_machine_hours_rate(
    cost_centre_name: str,
    budgeted_overhead: float,
    budgeted_machine_hours: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate OAR based on machine hours."""
    rate_per_mh = budgeted_overhead / budgeted_machine_hours if budgeted_machine_hours > 0 else 0

    await crud.record_analysis(
        db_session,
        user_id,
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="machine_hours",
        calculated_rate=rate_per_mh,
        base_units_used=budgeted_machine_hours,
        overhead_absorbed=budgeted_overhead,
    )

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
    cost_centre_name: str,
    budgeted_overhead: float,
    budgeted_direct_labour_hours: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate OAR based on direct labour hours."""
    rate_per_dlh = budgeted_overhead / budgeted_direct_labour_hours if budgeted_direct_labour_hours > 0 else 0

    await crud.record_analysis(
        db_session,
        user_id,
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="direct_labour_hours",
        calculated_rate=rate_per_dlh,
        base_units_used=budgeted_direct_labour_hours,
        overhead_absorbed=budgeted_overhead,
    )

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "direct_labour_hours",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_direct_labour_hours": budgeted_direct_labour_hours,
        "oar_per_direct_labour_hour": rate_per_dlh,
    }


@app.post("/labour-cost-rate")
async def calculate_labour_cost_rate(
    cost_centre_name: str,
    budgeted_overhead: float,
    budgeted_direct_labour_cost: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate OAR based on direct labour cost (percentage)."""
    rate_percentage = (budgeted_overhead / budgeted_direct_labour_cost * 100) if budgeted_direct_labour_cost > 0 else 0

    await crud.record_analysis(
        db_session,
        user_id,
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="direct_labour_cost",
        calculated_rate=rate_percentage,
        base_units_used=budgeted_direct_labour_cost,
        overhead_absorbed=budgeted_overhead,
    )

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "direct_labour_cost",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_direct_labour_cost": budgeted_direct_labour_cost,
        "oar_percentage": rate_percentage,
        "formula": (f"({budgeted_overhead} / {budgeted_direct_labour_cost}) × 100 = {rate_percentage}%"),
    }


@app.post("/material-cost-rate")
async def calculate_material_cost_rate(
    cost_centre_name: str,
    budgeted_overhead: float,
    budgeted_material_cost: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate OAR based on material cost (percentage)."""
    rate_percentage = (budgeted_overhead / budgeted_material_cost * 100) if budgeted_material_cost > 0 else 0

    await crud.record_analysis(
        db_session,
        user_id,
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="material_cost",
        calculated_rate=rate_percentage,
        base_units_used=budgeted_material_cost,
        overhead_absorbed=budgeted_overhead,
    )

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "material_cost",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_material_cost": budgeted_material_cost,
        "oar_percentage": rate_percentage,
    }


@app.post("/prime-cost-rate")
async def calculate_prime_cost_rate(
    cost_centre_name: str,
    budgeted_overhead: float,
    budgeted_prime_cost: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate OAR based on prime cost (percentage)."""
    rate_percentage = (budgeted_overhead / budgeted_prime_cost * 100) if budgeted_prime_cost > 0 else 0

    await crud.record_analysis(
        db_session,
        user_id,
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="prime_cost",
        calculated_rate=rate_percentage,
        base_units_used=budgeted_prime_cost,
        overhead_absorbed=budgeted_overhead,
    )

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "prime_cost",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_prime_cost": budgeted_prime_cost,
        "oar_percentage": rate_percentage,
    }


@app.post("/unit-rate")
async def calculate_unit_rate(
    cost_centre_name: str,
    budgeted_overhead: float,
    budgeted_units: float,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate OAR based on units produced."""
    rate_per_unit = budgeted_overhead / budgeted_units if budgeted_units > 0 else 0

    await crud.record_analysis(
        db_session,
        user_id,
        entity_name=cost_centre_name,
        cost_centre_name=cost_centre_name,
        rate_type="departmental",
        absorption_base="units",
        calculated_rate=rate_per_unit,
        base_units_used=budgeted_units,
        overhead_absorbed=budgeted_overhead,
    )

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_base": "units",
        "budgeted_overhead": budgeted_overhead,
        "budgeted_units": budgeted_units,
        "oar_per_unit": rate_per_unit,
    }


@app.post("/blanket-rate")
async def calculate_blanket_rate(
    entity_name: str,
    total_budgeted_overhead: float,
    total_base_units: float,
    absorption_base: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Calculate blanket OAR for entire factory."""
    blanket_rate = total_budgeted_overhead / total_base_units if total_base_units > 0 else 0

    await crud.record_analysis(
        db_session,
        user_id,
        entity_name=entity_name,
        cost_centre_name="entire_factory",
        rate_type="blanket",
        absorption_base=absorption_base,
        calculated_rate=blanket_rate,
        base_units_used=total_base_units,
        overhead_absorbed=total_budgeted_overhead,
    )

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
async def list_rates(
    cost_centre_id: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List overhead absorption rates."""
    return {"rates": await crud.list_rates(db_session, user_id, cost_centre_id)}


@app.get("/analyses")
async def list_analyses(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List OAR analyses."""
    return {"analyses": await crud.list_analyses(db_session, user_id)}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
