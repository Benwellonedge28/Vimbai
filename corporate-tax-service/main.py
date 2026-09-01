"""Vimbai Corporate Tax Service - Calculate and manage corporate tax obligations. Port: 8355"""
import os, uuid
from datetime import datetime, timezone
from typing import Any, Dict, List
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "corporate-tax-service"
PORT = int(os.getenv("PORT", "8355"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Corporate Tax Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="corporate-tax-service", instrument_app=app)
except ImportError:
    TRACER = None

class TaxComputation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    tax_year: int
    revenue: float
    deductible_expenses: float = 0
    capital_allowances: float = 0
    taxable_income: float = 0
    tax_rate: float = 25.0  # Zimbabwe corporate tax rate
    tax_owed: float = 0
    quarterly_estimates: List[float] = []
    credits: float = 0
    net_tax_liability: float = 0
    computed_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

_computations: Dict[str, List[TaxComputation]] = defaultdict(list)

def compute_tax(comp: TaxComputation) -> TaxComputation:
    comp.taxable_income = max(0, comp.revenue - comp.deductible_expenses - comp.capital_allowances)
    comp.tax_owed = comp.taxable_income * (comp.tax_rate / 100)
    comp.net_tax_liability = max(0, comp.tax_owed - comp.credits)
    if comp.quarterly_estimates and len(comp.quarterly_estimates) == 4:
        comp.quarterly_estimates = [comp.net_tax_liability / 4] * 4
    return comp

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/compute", response_model=TaxComputation)
async def compute_corporate_tax(comp: TaxComputation):
    comp = compute_tax(comp)
    _computations[comp.company_id].append(comp)
    logger.info("tax_computed", company_id=comp.company_id, year=comp.tax_year, liability=comp.net_tax_liability)
    return comp

@app.get("/computations/{company_id}")
async def get_computations(company_id: str):
    return {"company_id": company_id, "computations": _computations.get(company_id, []), "total": len(_computations.get(company_id, []))}

@app.get("/latest/{company_id}")
async def get_latest(company_id: str):
    comps = _computations.get(company_id, [])
    if not comps: raise HTTPException(status_code=404, detail="No tax computations found")
    return comps[-1]

@app.post("/provision/{company_id}")
async def calculate_provisional_tax(company_id: str, tax_year: int, annual_estimate: float, tax_rate: float = 25.0):
    quarterly = annual_estimate * (tax_rate / 100) / 4
    return {"company_id": company_id, "tax_year": tax_year, "annual_estimate": annual_estimate, "quarterly_payment": quarterly, "total_annual_provision": quarterly * 4, "due_dates": ["31 March", "30 June", "30 September", "31 December"]}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
