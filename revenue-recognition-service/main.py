"""Vimbai Revenue Recognition Service - IFRS 15 revenue recognition. Port: 8349"""
import os, uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "revenue-recognition-service"
PORT = int(os.getenv("PORT", "8349"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Revenue Recognition Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="revenue-recognition-service", instrument_app=app)
except ImportError:
    TRACER = None

class RecognitionMethod(str, Enum):
    POINT_IN_TIME = "point_in_time"; OVER_TIME = "over_time"

class PerformanceObligation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    description: str
    transaction_price: float
    standalone_selling_price: float = 0
    recognition_method: RecognitionMethod = RecognitionMethod.POINT_IN_TIME
    is_satisfied: bool = False
    revenue_recognized: float = 0

class RevenueContract(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    customer_name: str
    contract_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    total_transaction_price: float = 0
    obligations: List[PerformanceObligation] = []
    total_revenue_recognized: float = 0
    deferred_revenue: float = 0
    status: str = "active"

_contracts: Dict[str, List[RevenueContract]] = defaultdict(list)

def allocate_price(contract: RevenueContract):
    total_ssp = sum(o.standalone_selling_price for o in contract.obligations)
    if total_ssp > 0 and contract.total_transaction_price > 0:
        for o in contract.obligations:
            if o.standalone_selling_price > 0:
                o.transaction_price = contract.total_transaction_price * (o.standalone_selling_price / total_ssp)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/contracts", response_model=RevenueContract)
async def create_contract(contract: RevenueContract):
    contract.total_transaction_price = sum(o.transaction_price for o in contract.obligations)
    if any(o.standalone_selling_price > 0 for o in contract.obligations):
        allocate_price(contract)
    _contracts[contract.company_id].append(contract)
    logger.info("contract_created", company_id=contract.company_id, customer=contract.customer_name, value=contract.total_transaction_price)
    return contract

@app.post("/contracts/{contract_id}/recognize")
async def recognize_revenue(contract_id: str, obligation_id: str, amount: float = 0):
    for contracts in _contracts.values():
        for c in contracts:
            if c.id == contract_id:
                for o in c.obligations:
                    if o.id == obligation_id:
                        recog = amount if amount > 0 else o.transaction_price
                        o.revenue_recognized = min(o.transaction_price, o.revenue_recognized + recog)
                        o.is_satisfied = o.revenue_recognized >= o.transaction_price
                        c.total_revenue_recognized = sum(ob.revenue_recognized for ob in c.obligations)
                        c.deferred_revenue = c.total_transaction_price - c.total_revenue_recognized
                        return {"obligation_id": obligation_id, "recognized": o.revenue_recognized, "is_satisfied": o.is_satisfied, "contract_total_recognized": c.total_revenue_recognized, "deferred": c.deferred_revenue}
    raise HTTPException(status_code=404, detail="Contract or obligation not found")

@app.get("/contracts/{company_id}")
async def get_contracts(company_id: str):
    return {"company_id": company_id, "contracts": _contracts.get(company_id, []), "total": len(_contracts.get(company_id, []))}

@app.get("/summary/{company_id}")
async def revenue_summary(company_id: str):
    contracts = _contracts.get(company_id, [])
    total = sum(c.total_transaction_price for c in contracts)
    recognized = sum(c.total_revenue_recognized for c in contracts)
    deferred = sum(c.deferred_revenue for c in contracts)
    return {"company_id": company_id, "total_contracts": len(contracts), "total_contract_value": total, "revenue_recognized": recognized, "deferred_revenue": deferred}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
