"""
Vimbai Throughput Accounting Service
Theory of Constraints-based throughput accounting and product mix optimization.
Port: 8383
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "throughput_accounting_service" not in _sys.modules or not hasattr(
    _sys.modules.get("throughput_accounting_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("throughput_accounting_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["throughput_accounting_service"] = _pkg
    _sys.modules["throughput_accounting_service"].__path__ = [_HERE]

import os
import uuid
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field
from throughput_accounting_service import calc

SERVICE_NAME = "throughput-accounting-service"
PORT = int(os.getenv("PORT", "8383"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Throughput Accounting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class Product(BaseModel):
    name: str
    selling_price: float
    material_cost: float
    time_on_constraint: float  # minutes on bottleneck resource
    demand: int = 0


class ThroughputRequest(BaseModel):
    company_id: str
    operating_expenses: float
    products: List[Product]
    available_constraint_minutes: int = 480


class ThroughputResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    total_throughput: float
    operating_expenses: float
    net_profit: float
    roi: float
    product_ranking: List[Dict]
    optimal_mix: List[Dict]
    constraint_utilization: float


@app.get("/")
@app.get("/health")
async def health():
    return {
        "status": "healthy",
        "service": SERVICE_NAME,
        "version": "2.0.0",
        "calculation_engine": calc.engine_in_use(),
    }


@app.post("/analyze", response_model=ThroughputResult)
async def analyze_throughput(req: ThroughputRequest):
    result = calc.analyze(
        req.company_id,
        req.operating_expenses,
        [p.model_dump() for p in req.products],
        req.available_constraint_minutes,
    )

    return ThroughputResult(**result)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
