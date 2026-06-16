"""
FinAcc Depreciation Service
Manages depreciation calculations and journal entries.
"""

import os
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "depreciation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8035"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Depreciation Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class DepreciationMethod(str, Enum):
    STRAIGHT_LINE = "straight_line"
    REDUCING_BALANCE = "reducing_balance"
    SUM_OF_YEARS_DIGITS = "sum_of_years_digits"
    UNITS_OF_PRODUCTION = "units_of_production"
    MACRS = "macrs"


class AssetDepreciation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    asset_name: str
    cost: float
    residual_value: float
    useful_life_years: int
    depreciation_method: DepreciationMethod
    depreciation_rate: float = 0
    accumulated_depreciation: float = 0
    current_year_depreciation: float = 0
    net_book_value: float = 0
    period_end: Optional[datetime] = None
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class DepreciationSchedule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    asset_name: str
    cost: float
    residual_value: float
    depreciable_amount: float
    useful_life_years: int
    method: DepreciationMethod
    schedule: List[Dict[str, Any]] = []
    total_depreciation: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


# In-memory storage
asset_depreciations: Dict[str, AssetDepreciation] = {}
depreciation_schedules: Dict[str, DepreciationSchedule] = {}


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


async def call_audit_service(action: str, resource_type: str, resource_id: str, details: Dict[str, Any]):
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            await client.post(f"{AUDIT_SERVICE_URL}/audit", json={
                "action": action, "resource_type": resource_type, "resource_id": resource_id,
                "details": details, "timestamp": datetime.utcnow().isoformat()
            })
    except Exception:
        pass


def calculate_straight_line(cost: float, residual: float, life_years: int, year: int = 1) -> float:
    """Calculate straight line depreciation."""
    return (cost - residual) / life_years


def calculate_reducing_balance(cost: float, residual: float, life_years: int, accumulated: float = 0, rate: float = 0) -> float:
    """Calculate reducing balance depreciation."""
    if rate == 0:
        rate = (1 - (residual / cost) ** (1 / life_years)) * 100
    nbv = cost - accumulated
    return nbv * (rate / 100)


def calculate_sum_of_years(cost: float, residual: float, life_years: int, year: int) -> float:
    """Calculate sum of years digits depreciation."""
    depreciable = cost - residual
    sum_of_years = (life_years * (life_years + 1)) / 2
    return depreciable * ((life_years - year + 1) / sum_of_years)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Depreciation calculations and management"}


@app.post("/asset/register")
async def register_asset_depreciation(
    asset_id: str, asset_name: str, cost: float, residual_value: float,
    useful_life_years: int, depreciation_method: DepreciationMethod = DepreciationMethod.STRAIGHT_LINE
):
    """Register an asset for depreciation."""
    dep = AssetDepreciation(
        asset_id=asset_id, asset_name=asset_name, cost=cost, residual_value=residual_value,
        useful_life_years=useful_life_years, depreciation_method=depreciation_method,
        net_book_value=cost
    )
    asset_depreciations[asset_id] = dep
    await call_audit_service("CREATE", "depreciation", asset_id, {"cost": cost})
    return dep


@app.post("/schedule/generate")
async def generate_schedule(
    asset_id: str, asset_name: str, cost: float, residual_value: float,
    useful_life_years: int, method: DepreciationMethod = DepreciationMethod.STRAIGHT_LINE
):
    """Generate depreciation schedule."""
    schedule = []
    total_depreciation = 0
    accumulated = 0
    rate = 0

    if method == DepreciationMethod.REDUCING_BALANCE:
        rate = (1 - (residual_value / cost) ** (1 / useful_life_years)) * 100

    for year in range(1, useful_life_years + 1):
        if method == DepreciationMethod.STRAIGHT_LINE:
            amount = calculate_straight_line(cost, residual_value, useful_life_years, year)
        elif method == DepreciationMethod.REDUCING_BALANCE:
            amount = calculate_reducing_balance(cost, residual_value, useful_life_years, accumulated, rate)
        elif method == DepreciationMethod.SUM_OF_YEARS_DIGITS:
            amount = calculate_sum_of_years(cost, residual_value, useful_life_years, year)
        else:
            amount = calculate_straight_line(cost, residual_value, useful_life_years, year)

        accumulated += amount
        total_depreciation += amount
        nbv = cost - accumulated

        schedule.append({
            "year": year, "depreciation": round(amount, 2), "accumulated": round(accumulated, 2),
            "net_book_value": round(max(nbv, residual_value), 2)
        })

    depreciation_schedule = DepreciationSchedule(
        asset_id=asset_id, asset_name=asset_name, cost=cost, residual_value=residual_value,
        depreciable_amount=cost - residual_value, useful_life_years=useful_life_years,
        method=method, schedule=schedule, total_depreciation=total_depreciation
    )
    depreciation_schedules[asset_id] = depreciation_schedule

    return depreciation_schedule


@app.post("/calculate/{asset_id}")
async def calculate_depreciation(asset_id: str, period_end: datetime, units_produced: Optional[int] = None):
    """Calculate depreciation for a specific period."""
    dep = asset_depreciations.get(asset_id)
    if not dep:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Asset not found")

    if dep.depreciation_method == DepreciationMethod.STRAIGHT_LINE:
        dep.current_year_depreciation = calculate_straight_line(dep.cost, dep.residual_value, dep.useful_life_years)
    elif dep.depreciation_method == DepreciationMethod.REDUCING_BALANCE:
        dep.current_year_depreciation = calculate_reducing_balance(
            dep.cost, dep.residual_value, dep.useful_life_years, dep.accumulated_depreciation
        )
    elif dep.depreciation_method == DepreciationMethod.UNITS_OF_PRODUCTION and units_produced:
        total_units = dep.useful_life_years * 1000  # Example: 1000 units per year
        dep_per_unit = (dep.cost - dep.residual_value) / total_units
        dep.current_year_depreciation = dep_per_unit * units_produced
    else:
        dep.current_year_depreciation = calculate_straight_line(dep.cost, dep.residual_value, dep.useful_life_years)

    dep.accumulated_depreciation += dep.current_year_depreciation
    dep.net_book_value = dep.cost - dep.accumulated_depreciation
    dep.period_end = period_end

    # Create journal entry
    journal_entry = {
        "date": period_end,
        "description": f"Depreciation - {dep.asset_name}",
        "entries": [
            {"account_code": "6500", "description": "Depreciation Expense", "debit": dep.current_year_depreciation, "credit": 0},
            {"account_code": "1520", "description": "Accumulated Depreciation", "debit": 0, "credit": dep.current_year_depreciation},
        ],
        "reference": f"DEP-{asset_id[:8]}",
    }
    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    dep.journal_entry_id = result.get("id")

    await call_audit_service("CALCULATE", "depreciation", asset_id, {"amount": dep.current_year_depreciation})
    return dep


@app.post("/batch-calculate")
async def batch_calculate_depreciation(period_end: datetime):
    """Calculate depreciation for all assets."""
    total_depreciation = 0
    results = []

    for asset_id in asset_depreciations:
        dep = await calculate_depreciation(asset_id, period_end)
        total_depreciation += dep.current_year_depreciation
        results.append(dep)

    # Create consolidated journal entry
    journal_entry = {
        "date": period_end,
        "description": f"Depreciation for period ending {period_end.date()}",
        "entries": [
            {"account_code": "6500", "description": "Depreciation Expense", "debit": total_depreciation, "credit": 0},
            {"account_code": "1520", "description": "Accumulated Depreciation", "debit": 0, "credit": total_depreciation},
        ],
        "reference": f"DEP-BATCH-{period_end.strftime('%Y%m')}",
    }
    await call_accounting_service("POST", "/journal-entries", journal_entry)

    return {"assets": results, "total_depreciation": total_depreciation, "asset_count": len(results)}


@app.get("/assets")
async def list_assets():
    """List all assets with depreciation."""
    return {"assets": list(asset_depreciations.values())}


@app.get("/schedules")
async def list_schedules():
    """List all depreciation schedules."""
    return {"schedules": list(depreciation_schedules.values())}


@app.get("/summary")
async def get_depreciation_summary():
    """Get depreciation summary."""
    total_cost = sum(a.cost for a in asset_depreciations.values())
    total_accumulated = sum(a.accumulated_depreciation for a in asset_depreciations.values())
    total_nbv = sum(a.net_book_value for a in asset_depreciations.values())
    return {
        "total_assets": len(asset_depreciations),
        "total_cost": total_cost,
        "total_accumulated_depreciation": total_accumulated,
        "total_net_book_value": total_nbv
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)