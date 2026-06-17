"""
FinAcc Over/Under Absorption Service
Calculates over or under absorption of overheads.
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

SERVICE_NAME = "over-under-absorption-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8091"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Over/Under Absorption Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class OverUnderAbsorption(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_centre_name: str
    cost_centre_id: str
    period: str
    budgeted_overhead: float
    actual_overhead: float
    budgeted_base_units: float
    actual_base_units: float
    absorption_rate: float
    overhead_absorbed: float
    over_absorption: float = 0
    under_absorption: float = 0
    total_variance: float = 0
    cause: str = ""  # overhead_spending, volume, efficiency
    created_at: datetime = Field(default_factory=datetime.utcnow)


class VarianceAnalysis(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    cost_centre_name: str
    period: str
    overhead_absorbed: float
    actual_overhead: float
    spending_variance: float = 0  # budget vs actual overhead
    volume_variance: float = 0  # actual vs budgeted activity
    capacity_variance: float = 0  # idle capacity
    efficiency_variance: float = 0  # actual vs standard rate
    total_variance: float = 0
    over_or_under: str = ""
    created_at: datetime = Field(default_factory=datetime.utcnow)


absorption_records: List[OverUnderAbsorption] = []
variance_analyses: List[VarianceAnalysis] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Over/under absorption calculation"}


@app.post("/calculate")
async def calculate_over_under_absorption(
    cost_centre_name: str, cost_centre_id: str, period: str,
    budgeted_overhead: float, actual_overhead: float,
    budgeted_base_units: float, actual_base_units: float,
    absorption_rate: float
):
    """Calculate over or under absorption."""
    # Calculate overhead absorbed
    overhead_absorbed = actual_base_units * absorption_rate

    # Calculate variances
    variance = overhead_absorbed - actual_overhead
    over_absorption = variance if variance > 0 else 0
    under_absorption = abs(variance) if variance < 0 else 0

    record = OverUnderAbsorption(
        cost_centre_name=cost_centre_name, cost_centre_id=cost_centre_id, period=period,
        budgeted_overhead=budgeted_overhead, actual_overhead=actual_overhead,
        budgeted_base_units=budgeted_base_units, actual_base_units=actual_base_units,
        absorption_rate=absorption_rate, overhead_absorbed=overhead_absorbed,
        over_absorption=over_absorption, under_absorption=under_absorption,
        total_variance=abs(variance)
    )

    # Determine cause
    spending_variance = budgeted_overhead - actual_overhead
    if abs(spending_variance) > abs(variance - spending_variance):
        record.cause = "overhead_spending"
    else:
        record.cause = "volume"

    absorption_records.append(record)
    return record


@app.post("/simple-calculation")
async def simple_over_under_calculation(
    overhead_absorbed: float, actual_overhead: float
):
    """Simple over/under calculation."""
    variance = overhead_absorbed - actual_overhead
    is_over = variance > 0

    return {
        "overhead_absorbed": overhead_absorbed,
        "actual_overhead": actual_overhead,
        "variance": abs(variance),
        "status": "over_absorbed" if is_over else "under_absorbed",
        "interpretation": f"Overhead was {'over' if is_over else 'under'} absorbed by {abs(variance)}"
    }


@app.post("/spending-variance")
async def calculate_spending_variance(
    cost_centre_name: str, budgeted_overhead: float, actual_overhead: float
):
    """Calculate overhead spending variance."""
    spending_variance = budgeted_overhead - actual_overhead
    is_favorable = spending_variance > 0

    return {
        "cost_centre_name": cost_centre_name,
        "budgeted_overhead": budgeted_overhead,
        "actual_overhead": actual_overhead,
        "spending_variance": spending_variance,
        "is_favorable": is_favorable,
        "interpretation": f"Spending variance is {'favorable' if is_favorable else 'adverse'}: {abs(spending_variance)}"
    }


@app.post("/volume-variance")
async def calculate_volume_variance(
    cost_centre_name: str, absorption_rate: float,
    budgeted_base_units: float, actual_base_units: float
):
    """Calculate overhead volume variance."""
    budgeted_absorbed = absorption_rate * budgeted_base_units
    actual_absorbed = absorption_rate * actual_base_units
    volume_variance = actual_absorbed - budgeted_absorbed
    is_favorable = volume_variance > 0

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_rate": absorption_rate,
        "budgeted_base_units": budgeted_base_units,
        "actual_base_units": actual_base_units,
        "budgeted_absorbed": budgeted_absorbed,
        "actual_absorbed": actual_absorbed,
        "volume_variance": volume_variance,
        "is_favorable": is_favorable,
        "interpretation": f"Volume variance is {'favorable' if is_favorable else 'adverse'}: {abs(volume_variance)}"
    }


@app.post("/capacity-variance")
async def calculate_capacity_variance(
    cost_centre_name: str, absorption_rate: float,
    budgeted_capacity: float, actual_capacity: float
):
    """Calculate capacity variance (idle capacity)."""
    capacity_variance = absorption_rate * (actual_capacity - budgeted_capacity)
    is_favorable = capacity_variance > 0

    return {
        "cost_centre_name": cost_centre_name,
        "absorption_rate": absorption_rate,
        "budgeted_capacity": budgeted_capacity,
        "actual_capacity": actual_capacity,
        "idle_capacity": budgeted_capacity - actual_capacity,
        "capacity_variance": capacity_variance,
        "is_favorable": is_favorable,
        "interpretation": f"Capacity variance: {abs(capacity_variance)} ({'favorable' if is_favorable else 'adverse'})"
    }


@app.post("/comprehensive-analysis")
async def comprehensive_variance_analysis(
    cost_centre_name: str, period: str,
    budgeted_overhead: float, actual_overhead: float,
    budgeted_base_units: float, actual_base_units: float,
    absorption_rate: float
):
    """Comprehensive variance analysis."""
    # Overhead absorbed
    overhead_absorbed = absorption_rate * actual_base_units

    # Spending variance
    spending_variance = budgeted_overhead - actual_overhead
    spending_favorable = spending_variance > 0

    # Volume variance
    budgeted_absorbed = absorption_rate * budgeted_base_units
    volume_variance = overhead_absorbed - budgeted_absorbed
    volume_favorable = volume_variance > 0

    # Total variance
    total_variance = overhead_absorbed - actual_overhead
    is_over = total_variance > 0

    analysis = VarianceAnalysis(
        cost_centre_name=cost_centre_name, period=period,
        overhead_absorbed=overhead_absorbed, actual_overhead=actual_overhead,
        spending_variance=spending_variance, volume_variance=volume_variance,
        total_variance=total_variance,
        over_or_under="over" if is_over else "under"
    )
    variance_analyses.append(analysis)

    return {
        "cost_centre_name": cost_centre_name,
        "period": period,
        "budgeted_overhead": budgeted_overhead,
        "actual_overhead": actual_overhead,
        "overhead_absorbed": overhead_absorbed,
        "budgeted_base_units": budgeted_base_units,
        "actual_base_units": actual_base_units,
        "absorption_rate": absorption_rate,
        "spending_variance": spending_variance,
        "spending_favorable": spending_favorable,
        "volume_variance": volume_variance,
        "volume_favorable": volume_favorable,
        "total_variance": total_variance,
        "status": "over_absorbed" if is_over else "under_absorbed",
        "interpretation": {
            "spending": f"Spending variance {'favorable' if spending_favorable else 'adverse'} by {abs(spending_variance)}",
            "volume": f"Volume variance {'favorable' if volume_favorable else 'adverse'} by {abs(volume_variance)}",
            "total": f"Total {'over' if is_over else 'under'} absorption: {abs(total_variance)}"
        }
    }


@app.post("/adjust-over-under")
async def adjust_over_under(
    over_absorption: float, under_absorption: float,
    write_off_to_costing: bool = False
):
    """Determine treatment of over/under absorption."""
    net_variance = over_absorption - under_absorption

    if write_off_to_costing:
        treatment = "write_off_to_costing_profit_loss"
    else:
        treatment = "carry_forward_in_costing_records"

    return {
        "over_absorption": over_absorption,
        "under_absorption": under_absorption,
        "net_variance": net_variance,
        "treatment": treatment,
        "recommendation": "Over-absorption: reduce cost of production; Under-absorption: write off to P&L or carry forward"
    }


@app.get("/records")
async def list_records(cost_centre_id: Optional[str] = None, period: Optional[str] = None):
    """List over/under absorption records."""
    result = absorption_records
    if cost_centre_id:
        result = [r for r in result if r.cost_centre_id == cost_centre_id]
    if period:
        result = [r for r in result if r.period == period]
    return {"records": result}


@app.get("/analyses")
async def list_analyses():
    """List variance analyses."""
    return {"analyses": variance_analyses}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)
