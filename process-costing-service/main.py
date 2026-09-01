"""Vimbai Process Costing Service - Costing analysis and calculation. Port: 8340"""
import os, uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "process-costing-service"
PORT = int(os.getenv("PORT", "8340"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Process Costing Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="process-costing-service", instrument_app=app)
except ImportError:
    TRACER = None

class CostComponent(BaseModel):
    name: str
    amount: float
    cost_type: str = "direct"  # direct_materials, direct_labor, overhead, etc.

class CostCalculation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    product_or_process: str
    period: str = ""
    components: List[CostComponent] = []
    total_cost: float = 0
    unit_cost: float = 0
    quantity: int = 1
    notes: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

_calculations: Dict[str, List[CostCalculation]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/calculate", response_model=CostCalculation)
async def calculate_cost(calc: CostCalculation):
    calc.total_cost = sum(c.amount for c in calc.components)
    calc.unit_cost = calc.total_cost / max(1, calc.quantity)
    _calculations[calc.company_id].append(calc)
    logger.info("cost_calculated", company_id=calc.company_id, product=calc.product_or_process, total=calc.total_cost, unit=calc.unit_cost)
    return calc

@app.get("/calculations/{company_id}")
async def get_calculations(company_id: str, product: Optional[str] = None):
    calcs = _calculations.get(company_id, [])
    if product:
        calcs = [c for c in calcs if product.lower() in c.product_or_process.lower()]
    return {"company_id": company_id, "calculations": calcs, "total": len(calcs)}

@app.get("/breakdown/{company_id}/{calc_id}")
async def get_cost_breakdown(company_id: str, calc_id: str):
    for c in _calculations.get(company_id, []):
        if c.id == calc_id:
            by_type = defaultdict(float)
            for comp in c.components:
                by_type[comp.cost_type] += comp.amount
            return {"calc_id": calc_id, "total": c.total_cost, "unit_cost": c.unit_cost, "breakdown": dict(by_type), "components": c.components}
    raise HTTPException(status_code=404, detail="Calculation not found")

@app.get("/summary/{company_id}")
async def cost_summary(company_id: str):
    calcs = _calculations.get(company_id, [])
    if not calcs:
        return {"company_id": company_id, "total_calculations": 0, "total_cost": 0, "avg_unit_cost": 0}
    return {"company_id": company_id, "total_calculations": len(calcs), "total_cost": sum(c.total_cost for c in calcs), "avg_unit_cost": sum(c.unit_cost for c in calcs) / len(calcs)}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
