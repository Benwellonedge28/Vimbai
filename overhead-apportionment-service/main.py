"""
Vimbai Overhead Apportionment Service
Handles overhead apportionment to production cost centres.
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

SERVICE_NAME = "overhead-apportionment-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8071"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Overhead Apportionment Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class OverheadItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    overhead_name: str
    overhead_code: str
    total_amount: float
    basis: str  # floor_area, volume, nbv, personnel, requisitions, etc.
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ApportionedOverhead(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    overhead_id: str
    period: str
    cost_centre_apportionments: List[Dict[str, Any]] = []
    total_overhead: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


overhead_items: List[OverheadItem] = []
apportioned_overheads: List[ApportionedOverhead] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Overhead apportionment"}


@app.post("/overheads/add")
async def add_overhead_item(overhead_name: str, overhead_code: str, total_amount: float, basis: str):
    """Add an overhead item."""
    overhead = OverheadItem(
        overhead_name=overhead_name, overhead_code=overhead_code,
        total_amount=total_amount, basis=basis
    )
    overhead_items.append(overhead)
    return overhead


@app.post("/apportion")
async def apportion_overhead(
    overhead_id: str, period: str,
    cost_centre_data: List[Dict[str, Any]]
):
    """Apportion overhead to cost centres."""
    overhead = next((o for o in overhead_items if o.id == overhead_id), None)
    if not overhead:
        return {"error": "Overhead not found"}

    result = ApportionedOverhead(
        overhead_id=overhead_id, period=period,
        total_overhead=overhead.total_amount
    )

    basis_field = overhead.basis
    total_basis = sum(c.get(basis_field, 0) for c in cost_centre_data)

    for centre in cost_centre_data:
        basis_value = centre.get(basis_field, 0)
        if total_basis > 0:
            proportion = basis_value / total_basis
            apportioned = overhead.total_amount * proportion
            result.cost_centre_apportionments.append({
                "cost_centre_id": centre["cost_centre_id"],
                "cost_centre_name": centre["cost_centre_name"],
                "basis_value": basis_value,
                "basis": overhead.basis,
                "proportion": proportion,
                "apportioned_amount": apportioned
            })

    apportioned_overheads.append(result)
    return result


@app.post("/batch-apportion")
async def batch_apportion_overheads(
    period: str, cost_centre_data: List[Dict[str, Any]]
):
    """Apportion all overheads for a period."""
    results = []

    for overhead in overhead_items:
        result = await apportion_overhead(overhead.id, period, cost_centre_data)
        results.append(result)

    return {"period": period, "apportionment_results": results}


@app.get("/overheads")
async def list_overhead_items():
    """List overhead items."""
    return {"overheads": overhead_items}


@app.get("/apportionments")
async def list_apportionments(
    overhead_id: Optional[str] = None,
    period: Optional[str] = None
):
    """List apportionment results."""
    result = apportioned_overheads
    if overhead_id:
        result = [a for a in result if a.overhead_id == overhead_id]
    if period:
        result = [a for a in result if a.period == period]
    return {"apportionments": result}


@app.get("/cost-centre-summary/{period}")
async def get_cost_centre_overhead_summary(period: str):
    """Get total overhead apportioned to each cost centre."""
    centre_totals = {}

    for apportionment in apportionment_overheads:
        if apportionment.period == period:
            for centre in apportionment.cost_centre_apportionments:
                centre_id = centre["cost_centre_id"]
                if centre_id not in centre_totals:
                    centre_totals[centre_id] = {
                        "cost_centre_id": centre_id,
                        "cost_centre_name": centre["cost_centre_name"],
                        "total_overhead": 0,
                        "breakdown": []
                    }
                centre_totals[centre_id]["total_overhead"] += centre["apportioned_amount"]
                centre_totals[centre_id]["breakdown"].append({
                    "overhead_id": apportionment.overhead_id,
                    "apportioned_amount": centre["apportioned_amount"],
                    "basis": centre["basis"]
                })

    return {"period": period, "cost_centre_summary": list(centre_totals.values())}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)