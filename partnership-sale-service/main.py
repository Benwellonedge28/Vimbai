"""
FinAcc Partnership Sale Service
Sale of partnership business to a limited company.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "partnership-sale-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8046"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Partnership Sale Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class AssetSale(BaseModel):
    asset_id: str
    asset_name: str
    book_value: float
    sale_price: float
    gain_loss: float = 0


class SaleReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    partnership_id: str
    buyer_company_id: str
    sale_date: datetime
    total_purchase_price: float
    assets_sold: List[AssetSale] = []
    liabilities_taken_over: float = 0
    goodwill_amount: float = 0
    total_realization: float = 0
    partner_distributions: Dict[str, float] = {}
    journal_entry_ids: List[str] = []
    status: str = "pending"
    created_at: datetime = Field(default_factory=datetime.utcnow)


sales: List[SaleReport] = []


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception:
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Partnership sale to limited company"}


@app.post("/sale/process")
async def process_sale(
    partnership_id: str, buyer_company_id: str, sale_date: datetime,
    purchase_price: float, assets: List[Dict[str, Any]], liabilities_taken: float = 0,
    partner_shares: Dict[str, float], goodwill: float = 0
):
    """Process sale of partnership to limited company."""
    report = SaleReport(
        partnership_id=partnership_id, buyer_company_id=buyer_company_id,
        sale_date=sale_date, total_purchase_price=purchase_price,
        liabilities_taken_over=liabilities_taken, goodwill_amount=goodwill
    )

    total_gain = 0
    for asset in assets:
        sale = AssetSale(
            asset_id=asset["id"], asset_name=asset["name"],
            book_value=asset["book_value"], sale_price=asset["sale_price"]
        )
        sale.gain_loss = sale.sale_price - sale.book_value
        total_gain += sale.gain_loss
        report.assets_sold.append(sale)

    report.total_realization = purchase_price
    report.partner_distributions = partner_shares

    # Create journal entries
    entries = [
        {"date": sale_date, "description": "Sale proceeds", "entries": [
            {"account_code": "1000", "debit": purchase_price, "credit": 0},
            {"account_code": "3000", "debit": 0, "credit": total_gain},
        ]}
    ]

    for entry in entries:
        result = await call_accounting_service("POST", "/journal-entries", entry)
        report.journal_entry_ids.append(result.get("id", ""))

    report.status = "completed"
    sales.append(report)
    return report


@app.get("/sales")
async def list_sales():
    return {"sales": sales}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)