"""Vimbai Cost Accounting Service - Standard costing, variance analysis, cost allocation. Port: 8347"""
import os, uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "cost-accounting-service"
PORT = int(os.getenv("PORT", "8347"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Cost Accounting Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="cost-accounting-service", instrument_app=app)
except ImportError:
    TRACER = None

class VarianceType(str, Enum):
    FAVORABLE = "favorable"; UNFAVORABLE = "unfavorable"

class StandardCost(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    product_name: str
    direct_materials_std: float
    direct_labor_std: float
    overhead_std: float
    standard_cost_per_unit: float = 0
    actual_materials: float = 0
    actual_labor: float = 0
    actual_overhead: float = 0
    actual_cost_per_unit: float = 0
    material_variance: float = 0
    labor_variance: float = 0
    overhead_variance: float = 0
    total_variance: float = 0
    units_produced: int = 0

_costs: Dict[str, List[StandardCost]] = defaultdict(list)

def calc_variance(actual: float, standard: float) -> tuple:
    diff = actual - standard
    return diff, VarianceType.FAVORABLE if diff <= 0 else VarianceType.UNFAVORABLE

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/standards", response_model=StandardCost)
async def set_standard_cost(cost: StandardCost):
    cost.standard_cost_per_unit = cost.direct_materials_std + cost.direct_labor_std + cost.overhead_std
    if cost.units_produced > 0:
        cost.actual_cost_per_unit = (cost.actual_materials + cost.actual_labor + cost.actual_overhead) / cost.units_produced
        cost.material_variance, _ = calc_variance(cost.actual_materials, cost.direct_materials_std * cost.units_produced)
        cost.labor_variance, _ = calc_variance(cost.actual_labor, cost.direct_labor_std * cost.units_produced)
        cost.overhead_variance, _ = calc_variance(cost.actual_overhead, cost.overhead_std * cost.units_produced)
        cost.total_variance = cost.material_variance + cost.labor_variance + cost.overhead_variance
    _costs[cost.company_id].append(cost)
    return cost

@app.get("/variances/{company_id}")
async def get_variances(company_id: str):
    costs = _costs.get(company_id, [])
    return {"company_id": company_id, "items": costs, "total": len(costs), "total_variance": sum(c.total_variance for c in costs)}

@app.get("/standards/{company_id}")
async def get_standards(company_id: str):
    return {"company_id": company_id, "standards": _costs.get(company_id, [])}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
