"""
Vimbai Authorized Share Capital Service
Manages authorized share capital, share classes, and issuance records.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "authorized-share-capital-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8017"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Authorized Share Capital Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class ShareClass(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str  # ordinary, preference, founder, treasury
    authorized_shares: int
    issued_shares: int = 0
    par_value: float = 0.0
    voting_rights: str = "ordinary"  # ordinary, preferential, none
    dividend_rate: float = 0.0  # for preference shares
    rights: str = ""  # description of class rights
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class ShareIssuance(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    share_class_id: str
    number_of_shares: int
    issue_price: float
    total_proceeds: float = 0.0
    issue_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    issued_to: str = ""
    notes: str = ""


class ShareBuyback(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    share_class_id: str
    number_of_shares: int
    buyback_price: float
    total_cost: float = 0.0
    buyback_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    notes: str = ""


share_classes: List[ShareClass] = []
issuances: List[ShareIssuance] = []
buybacks: List[ShareBuyback] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/share-classes", response_model=ShareClass)
async def create_share_class(
    name: str,
    authorized_shares: int,
    par_value: float = 0.0,
    voting_rights: str = "ordinary",
    dividend_rate: float = 0.0,
    rights: str = "",
):
    """Create a share class."""
    valid_voting = ["ordinary", "preferential", "none"]
    if voting_rights not in valid_voting:
        raise HTTPException(status_code=400, detail=f"Invalid voting rights. Must be one of {valid_voting}")

    share_class = ShareClass(
        name=name,
        authorized_shares=authorized_shares,
        par_value=par_value,
        voting_rights=voting_rights,
        dividend_rate=dividend_rate,
        rights=rights,
    )
    share_classes.append(share_class)
    logger.info("Share class created", class_id=share_class.id, name=name, authorized=authorized_shares)
    return share_class


@app.get("/share-classes", response_model=List[ShareClass])
async def list_share_classes():
    """List all share classes."""
    return share_classes


@app.post("/share-classes/{class_id}/issue", response_model=ShareIssuance)
async def issue_shares(class_id: str, number_of_shares: int, issue_price: float, issued_to: str = "", notes: str = ""):
    """Issue shares from a share class."""
    share_class = next((sc for sc in share_classes if sc.id == class_id), None)
    if not share_class:
        raise HTTPException(status_code=404, detail="Share class not found")

    if share_class.issued_shares + number_of_shares > share_class.authorized_shares:
        raise HTTPException(status_code=400, detail="Issuance exceeds authorized shares")

    total_proceeds = number_of_shares * issue_price
    issuance = ShareIssuance(
        share_class_id=class_id,
        number_of_shares=number_of_shares,
        issue_price=issue_price,
        total_proceeds=total_proceeds,
        issued_to=issued_to,
        notes=notes,
    )
    share_class.issued_shares += number_of_shares
    issuances.append(issuance)
    logger.info("Shares issued", class_id=class_id, shares=number_of_shares, proceeds=total_proceeds)
    return issuance


@app.get("/share-classes/{class_id}/issuances", response_model=List[ShareIssuance])
async def list_issuances(class_id: str):
    """List share issuances for a class."""
    return [i for i in issuances if i.share_class_id == class_id]


@app.post("/share-classes/{class_id}/buyback", response_model=ShareBuyback)
async def buyback_shares(class_id: str, number_of_shares: int, buyback_price: float, notes: str = ""):
    """Buy back shares from a share class."""
    share_class = next((sc for sc in share_classes if sc.id == class_id), None)
    if not share_class:
        raise HTTPException(status_code=404, detail="Share class not found")
    if number_of_shares > share_class.issued_shares:
        raise HTTPException(status_code=400, detail="Buyback exceeds issued shares")

    total_cost = number_of_shares * buyback_price
    buyback = ShareBuyback(
        share_class_id=class_id,
        number_of_shares=number_of_shares,
        buyback_price=buyback_price,
        total_cost=total_cost,
        notes=notes,
    )
    share_class.issued_shares -= number_of_shares
    buybacks.append(buyback)
    logger.info("Shares bought back", class_id=class_id, shares=number_of_shares, cost=total_cost)
    return buyback


@app.get("/summary")
async def capital_summary():
    """Get authorized share capital summary."""
    return {
        "total_classes": len(share_classes),
        "total_authorized": sum(sc.authorized_shares for sc in share_classes),
        "total_issued": sum(sc.issued_shares for sc in share_classes),
        "total_proceeds": sum(i.total_proceeds for i in issuances),
        "total_buyback_cost": sum(b.total_cost for b in buybacks),
        "by_class": [
            {
                "name": sc.name,
                "authorized": sc.authorized_shares,
                "issued": sc.issued_shares,
                "available": sc.authorized_shares - sc.issued_shares,
            }
            for sc in share_classes
        ],
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
