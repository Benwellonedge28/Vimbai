"""
Vimbai Partnership Dissolution Service
Handles complete dissolution of partnerships.
"""

import os
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "partnership-dissolution-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8045"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Partnership Dissolution Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class DissolutionReason(str, Enum):
    MUTUAL_AGREEMENT = "mutual_agreement"
    EXPIRY_OF_TERM = "expiry_of_term"
    COMPLETION_OF_ADVENTURE = "completion_of_adventure"
    DEATH_OF_PARTNER = "death_of_partner"
    INSOLVENCY = "insolvency"
    COURT_ORDER = "court_order"


class AssetRealization(BaseModel):
    asset_id: str
    asset_name: str
    book_value: float
    sale_proceeds: float
    profit: float = 0
    loss: float = 0


class CreditorSettlement(BaseModel):
    creditor_id: str
    creditor_name: str
    amount_owed: float
    amount_paid: float
    discount_received: float = 0


class PartnerSettlement(BaseModel):
    partner_id: str
    partner_name: str
    capital_balance: float
    current_account_balance: float
    share_of_profit_loss: float
    total_due: float


class DissolutionReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    partnership_id: str
    dissolution_date: datetime
    reason: DissolutionReason
    total_assets_realized: float = 0
    total_liabilities_paid: float = 0
    total_creditors: float = 0
    total_partners_capitals: float = 0
    realization_profit: float = 0
    realization_loss: float = 0
    assets: List[AssetRealization] = []
    creditors: List[CreditorSettlement] = []
    partners: List[PartnerSettlement] = []
    journal_entry_ids: List[str] = []
    status: str = "pending"
    created_at: datetime = Field(default_factory=datetime.utcnow)


dissolutions: List[DissolutionReport] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Partnership dissolution service"}


@app.post("/dissolve")
async def create_dissolution(
    partnership_id: str, dissolution_date: datetime, reason: DissolutionReason,
    assets: List[Dict[str, Any]], creditors: List[Dict[str, Any]], partners: List[Dict[str, Any]]
):
    """Process partnership dissolution."""
    report = DissolutionReport(
        partnership_id=partnership_id, dissolution_date=dissolution_date, reason=reason
    )

    # Asset realizations
    for asset in assets:
        realization = AssetRealization(
            asset_id=asset["asset_id"], asset_name=asset["asset_name"],
            book_value=asset["book_value"], sale_proceeds=asset["sale_proceeds"]
        )
        if realization.sale_proceeds > realization.book_value:
            realization.profit = realization.sale_proceeds - realization.book_value
            report.realization_profit += realization.profit
        else:
            realization.loss = realization.book_value - realization.sale_proceeds
            report.realization_loss += realization.loss
        report.assets.append(realization)
        report.total_assets_realized += realization.sale_proceeds

    # Creditor settlements
    for creditor in creditors:
        settlement = CreditorSettlement(
            creditor_id=creditor["id"], creditor_name=creditor["name"],
            amount_owed=creditor["amount"], amount_paid=creditor.get("paid", creditor["amount"]),
            discount_received=creditor.get("discount", 0)
        )
        report.creditors.append(settlement)
        report.total_creditors += settlement.amount_paid

    # Partner settlements
    for partner in partners:
        settlement = PartnerSettlement(
            partner_id=partner["id"], partner_name=partner["name"],
            capital_balance=partner["capital"], current_account_balance=partner.get("current", 0),
            share_of_profit_loss=partner.get("share", 0), total_due=0
        )
        settlement.total_due = settlement.capital_balance + settlement.current_account_balance + settlement.share_of_profit_loss
        report.partners.append(settlement)
        report.total_partners_capitals += settlement.total_due

    # Calculate net position
    net = report.total_assets_realized - report.total_creditors - report.total_partners_capitals
    report.status = "completed"
    dissolutions.append(report)
    return report


@app.get("/dissolutions")
async def list_dissolutions():
    return {"dissolutions": dissolutions}


@app.get("/dissolutions/{dissolution_id}")
async def get_dissolution(dissolution_id: str):
    return next((d for d in dissolutions if d.id == dissolution_id), {"error": "Not found"})


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)