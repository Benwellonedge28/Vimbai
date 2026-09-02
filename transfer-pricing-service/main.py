"""
Vimbai Transfer Pricing Service
OECD-aligned transfer pricing analysis with comparable pricing and documentation.
Port: 8379
"""

import os
import uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "transfer-pricing-service"
PORT = int(os.getenv("PORT", "8379"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Transfer Pricing Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class PricingMethod(str, Enum):
    CUP = "comparable_uncontrolled_price"
    RESALE = "resale_price"
    COST_PLUS = "cost_plus"
    TNMM = "transactional_net_margin"
    PROFIT_SPLIT = "profit_split"


class IntercompanyTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    product_service: str
    selling_entity: str
    buying_entity: str
    transaction_value: float
    cost_of_goods: float = 0
    arm_length_range_min: float = 0
    arm_length_range_max: float = 0
    method: PricingMethod = PricingMethod.CUP


class TPAnalysisRequest(BaseModel):
    company_id: str
    transactions: List[IntercompanyTransaction]
    benchmark_data: List[Dict] = []


class TPAnalysisResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    analysis_date: str
    compliant_transactions: int
    non_compliant_transactions: int
    total_adjustment_needed: float
    transactions: List[Dict]
    documentation_required: List[str] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/analyze", response_model=TPAnalysisResult)
async def analyze_transfer_pricing(req: TPAnalysisRequest):
    compliant = 0
    non_compliant = 0
    total_adjustment = 0
    tx_results = []

    for tx in req.transactions:
        is_compliant = tx.arm_length_range_min <= tx.transaction_value <= tx.arm_length_range_max
        if is_compliant:
            compliant += 1
            adjustment = 0
        else:
            non_compliant += 1
            if tx.transaction_value < tx.arm_length_range_min:
                adjustment = tx.arm_length_range_min - tx.transaction_value
            else:
                adjustment = tx.arm_length_range_max - tx.transaction_value
            total_adjustment += abs(adjustment)

        tx_results.append(
            {
                "id": tx.id,
                "product_service": tx.product_service,
                "selling_entity": tx.selling_entity,
                "buying_entity": tx.buying_entity,
                "transaction_value": tx.transaction_value,
                "arm_length_range": [tx.arm_length_range_min, tx.arm_length_range_max],
                "method": tx.method.value,
                "compliant": is_compliant,
                "adjustment_needed": round(adjustment, 2),
            }
        )

    return TPAnalysisResult(
        company_id=req.company_id,
        analysis_date=datetime.now(timezone.utc).isoformat(),
        compliant_transactions=compliant,
        non_compliant_transactions=non_compliant,
        total_adjustment_needed=round(total_adjustment, 2),
        transactions=tx_results,
        documentation_required=[
            "Master File (OECD)",
            "Local File for each jurisdiction",
            "Country-by-Country report (if revenue threshold met)",
            "Benchmarking study supporting selected method",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
