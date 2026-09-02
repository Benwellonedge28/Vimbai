"""
Vimbai Group Tax Service
Group tax consolidation, intercompany elimination, and group-level tax planning.
Port: 8378
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "group-tax-service"
PORT = int(os.getenv("PORT", "8378"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Group Tax Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class SubsidiaryTax(BaseModel):
    entity_id: str; entity_name: str; jurisdiction: str
    pre_tax_income: float; tax_paid: float; tax_rate: float
    loss_carryforward: float = 0

class GroupTaxRequest(BaseModel):
    group_id: str; fiscal_year: int
    subsidiaries: List[SubsidiaryTax]
    group_tax_rate: float = 0.25
    intercompany_eliminations: float = 0

class GroupTaxResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    group_id: str; fiscal_year: int
    consolidated_income: float; consolidated_tax: float
    total_tax_already_paid: float; net_group_tax: float
    tax_savings_from_consolidation: float
    subsidiary_summary: List[Dict] = []

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/consolidate", response_model=GroupTaxResult)
async def consolidate_group_tax(req: GroupTaxRequest):
    total_income = sum(s.pre_tax_income for s in req.subsidiaries) - req.intercompany_eliminations
    total_tax_paid = sum(s.tax_paid for s in req.subsidiaries)
    
    # Offsetting losses within the group
    profitable = [s for s in req.subsidiaries if s.pre_tax_income > 0]
    loss_making = [s for s in req.subsidiaries if s.pre_tax_income <= 0]
    
    total_losses = sum(abs(s.pre_tax_income) for s in loss_making)
    consolidated_income = max(total_income, 0)
    consolidated_tax = consolidated_income * req.group_tax_rate
    
    # What they would have paid without consolidation
    standalone_tax = sum(max(s.pre_tax_income, 0) * s.tax_rate for s in req.subsidiaries)
    savings = standalone_tax - consolidated_tax - sum(max(s.pre_tax_income, 0) * s.tax_rate - s.tax_paid for s in req.subsidiaries if s.pre_tax_income > 0)
    savings = max(savings, 0)
    
    summary = [{
        "entity_id": s.entity_id, "name": s.entity_name,
        "jurisdiction": s.jurisdiction, "income": s.pre_tax_income,
        "tax_paid": s.tax_paid, "rate": s.tax_rate,
        "loss_carryforward": s.loss_carryforward
    } for s in req.subsidiaries]
    
    return GroupTaxResult(
        group_id=req.group_id, fiscal_year=req.fiscal_year,
        consolidated_income=round(consolidated_income, 2),
        consolidated_tax=round(consolidated_tax, 2),
        total_tax_already_paid=round(total_tax_paid, 2),
        net_group_tax=round(max(consolidated_tax - total_tax_paid, 0), 2),
        tax_savings_from_consolidation=round(savings, 2),
        subsidiary_summary=summary
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
