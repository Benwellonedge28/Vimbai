"""
Vimbai R&D Tax Credit Service
R&D tax credit calculation, qualifying expenditure tracking, and claim preparation.
Port: 8377
"""

import importlib.util
import os
import sys
import uuid
from datetime import datetime, timezone
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

# The calculator sits beside this file. It is loaded under its own module name so
# that a process holding two services at once cannot hand this one the other's
# calculator; a plain `import calc` would.
_CALC_DIR = os.path.dirname(os.path.abspath(__file__))
_spec = importlib.util.spec_from_file_location("r_and_d_tax_calc", os.path.join(_CALC_DIR, "calc.py"))
calc = importlib.util.module_from_spec(_spec)
sys.modules["r_and_d_tax_calc"] = calc
_spec.loader.exec_module(calc)

SERVICE_NAME = "r-and-d-tax-service"
PORT = int(os.getenv("PORT", "8377"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai R&D Tax Credit Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class RDExpenditure(BaseModel):
    category: str  # wages, supplies, contract_research, overhead
    description: str
    amount: float
    qualifies: bool = True


class RDClaimRequest(BaseModel):
    company_id: str
    fiscal_year: int
    expenditures: List[RDExpenditure]
    credit_rate: float = 0.15  # 15% R&D credit rate
    alternative_rate: float = 0.20  # alternative simplified credit


class RDClaimResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    fiscal_year: int
    total_expenditure: float
    qualifying_expenditure: float
    non_qualifying: float
    credit_regular: float
    credit_alternative: float
    recommended_method: str
    expenditure_breakdown: Dict[str, float] = {}


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/calculate", response_model=RDClaimResult)
async def calculate_credit(req: RDClaimRequest):
    result = calc.credit_claim(
        req.company_id,
        req.fiscal_year,
        [e.model_dump() for e in req.expenditures],
        req.credit_rate,
        req.alternative_rate,
    )
    logger.info("R&D credit calculated", company=req.company_id, method=result["recommended_method"])
    return RDClaimResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
