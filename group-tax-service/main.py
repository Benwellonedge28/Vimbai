"""
Vimbai Group Tax Service
Group tax consolidation, intercompany elimination, and group-level tax planning.
Port: 8378
"""

import importlib.util
import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("group_tax_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["group_tax_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "group-tax-service"
PORT = int(os.getenv("PORT", "8378"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Group Tax Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class SubsidiaryTax(BaseModel):
    entity_id: str
    entity_name: str
    jurisdiction: str
    pre_tax_income: float
    tax_paid: float
    tax_rate: float
    loss_carryforward: float = 0


class GroupTaxRequest(BaseModel):
    group_id: str
    fiscal_year: int
    subsidiaries: List[SubsidiaryTax]
    group_tax_rate: float = 0.25
    intercompany_eliminations: float = 0


class GroupTaxResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    group_id: str
    fiscal_year: int
    consolidated_income: float
    consolidated_tax: float
    total_tax_already_paid: float
    net_group_tax: float
    tax_savings_from_consolidation: float
    subsidiary_summary: List[Dict] = []


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/consolidate", response_model=GroupTaxResult)
async def consolidate_group_tax(req: GroupTaxRequest):
    result = calc.consolidate(
        req.group_id,
        req.fiscal_year,
        [s.model_dump() for s in req.subsidiaries],
        req.group_tax_rate,
        req.intercompany_eliminations,
    )
    logger.info("Group tax consolidated", group=req.group_id, year=req.fiscal_year)
    return GroupTaxResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
