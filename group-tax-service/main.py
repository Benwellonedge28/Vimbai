"""Vimbai Group Tax Service - Group/consolidated tax management. Port: 8356"""
import os, uuid
from datetime import datetime, timezone
from typing import Any, Dict, List
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "group-tax-service"
PORT = int(os.getenv("PORT", "8356"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Group Tax Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="group-tax-service", instrument_app=app)
except ImportError:
    TRACER = None

class SubsidiaryTax(BaseModel):
    subsidiary_id: str
    subsidiary_name: str
    taxable_income: float
    tax_rate: float = 25.0
    tax_liability: float = 0

class GroupTaxCalculation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    group_id: str
    tax_year: int
    subsidiaries: List[SubsidiaryTax] = []
    consolidated_income: float = 0
    group_tax_rate: float = 25.0
    total_tax_liability: float = 0
    intercompany_eliminations: float = 0
    computed_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

_calcs: Dict[str, List[GroupTaxCalculation]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/compute", response_model=GroupTaxCalculation)
async def compute_group_tax(calc: GroupTaxCalculation):
    for sub in calc.subsidiaries:
        sub.tax_liability = sub.taxable_income * (sub.tax_rate / 100)
    calc.consolidated_income = sum(s.taxable_income for s in calc.subsidiaries) - calc.intercompany_eliminations
    calc.total_tax_liability = max(0, calc.consolidated_income) * (calc.group_tax_rate / 100)
    _calcs[calc.group_id].append(calc)
    logger.info("group_tax_computed", group_id=calc.group_id, year=calc.tax_year, liability=calc.total_tax_liability)
    return calc

@app.get("/calculations/{group_id}")
async def get_calculations(group_id: str):
    return {"group_id": group_id, "calculations": _calcs.get(group_id, []), "total": len(_calcs.get(group_id, []))}

@app.get("/breakdown/{group_id}")
async def tax_breakdown(group_id: str):
    calcs = _calcs.get(group_id, [])
    if not calcs: raise HTTPException(status_code=404, detail="No calculations found")
    latest = calcs[-1]
    return {"group_id": group_id, "subsidiary_breakdown": [{"name": s.subsidiary_name, "income": s.taxable_income, "rate": s.tax_rate, "liability": s.tax_liability} for s in latest.subsidiaries], "total_consolidated": latest.consolidated_income, "total_liability": latest.total_tax_liability}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
