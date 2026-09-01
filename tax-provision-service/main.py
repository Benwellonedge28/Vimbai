"""Vimbai Tax Provision Service - ASC 740 tax provision calculations. Port: 8358"""
import os, uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "tax-provision-service"
PORT = int(os.getenv("PORT", "8358"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Tax Provision Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="tax-provision-service", instrument_app=app)
except ImportError:
    TRACER = None

class TaxProvision(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str
    pre_tax_income: float
    statutory_rate: float = 25.0
    permanent_differences: float = 0
    temporary_differences: float = 0
    current_tax_expense: float = 0
    deferred_tax_expense: float = 0
    total_tax_expense: float = 0
    effective_tax_rate: float = 0
    computed_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))

_provisions: Dict[str, List[TaxProvision]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/calculate", response_model=TaxProvision)
async def calculate_provision(prov: TaxProvision):
    taxable = max(0, prov.pre_tax_income - prov.permanent_differences)
    prov.current_tax_expense = taxable * (prov.statutory_rate / 100)
    prov.deferred_tax_expense = prov.temporary_differences * (prov.statutory_rate / 100)
    prov.total_tax_expense = prov.current_tax_expense + prov.deferred_tax_expense
    prov.effective_tax_rate = (prov.total_tax_expense / max(1, prov.pre_tax_income)) * 100 if prov.pre_tax_income != 0 else 0
    _provisions[prov.company_id].append(prov)
    logger.info("provision_calculated", company_id=prov.company_id, total=prov.total_tax_expense, etr=prov.effective_tax_rate)
    return prov

@app.get("/provisions/{company_id}")
async def get_provisions(company_id: str):
    return {"company_id": company_id, "provisions": _provisions.get(company_id, []), "total": len(_provisions.get(company_id, []))}

@app.get("/latest/{company_id}")
async def get_latest(company_id: str):
    provs = _provisions.get(company_id, [])
    if not provs: raise HTTPException(status_code=404, detail="No provisions found")
    return provs[-1]

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
