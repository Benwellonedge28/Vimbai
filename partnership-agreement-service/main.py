"""
Vimbai Partnership Agreement Service
Manages partnership agreements and deeds.
"""

import os
import uuid
from datetime import datetime
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "partnership-agreement-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8042"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Partnership Agreement Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class Partner(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    address: str
    contribution: float
    profit_sharing_ratio: float
    is_active: bool = True


class PartnershipAgreement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    agreement_number: str
    partnership_name: str
    partners: List[Partner] = []
    start_date: datetime
    end_date: Optional[datetime] = None
    duration_years: Optional[int] = None
    business_nature: str
    capital_amount: float = 0
    profit_sharing_basis: str = "ratio"  # equal, ratio, capital_based
    drawings_allowed: bool = True
    max_drawings: Optional[float] = None
    interest_on_capital_rate: float = 0
    interest_on_drawings_rate: float = 0
    guaranteed_salary: bool = False
    commission_allowed: bool = False
    admission_new_partner: bool = True
    retirement_conditions: str = ""
    dissolution_conditions: str = ""
    dispute_resolution: str = ""
    is_active: bool = True
    agreement_document: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)


agreements: Dict[str, PartnershipAgreement] = {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Partnership agreement management"}


@app.post("/agreements", response_model=PartnershipAgreement, status_code=status.HTTP_201_CREATED)
async def create_agreement(data: PartnershipAgreement):
    """Create a new partnership agreement."""
    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()
    data.updated_at = datetime.utcnow()
    data.capital_amount = sum(p.contribution for p in data.partners)
    agreements[data.id] = data
    return data


@app.get("/agreements/{agreement_id}")
async def get_agreement(agreement_id: str):
    """Get agreement details."""
    if agreement_id not in agreements:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Agreement not found")
    return agreements[agreement_id]


@app.get("/agreements")
async def list_agreements(is_active: Optional[bool] = None):
    """List all agreements."""
    result = list(agreements.values())
    if is_active is not None:
        result = [a for a in result if a.is_active == is_active]
    return {"agreements": result, "count": len(result)}


@app.put("/agreements/{agreement_id}")
async def update_agreement(agreement_id: str, data: Dict[str, Any]):
    """Update agreement."""
    if agreement_id not in agreements:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Agreement not found")
    for key, value in data.items():
        if hasattr(agreements[agreement_id], key):
            setattr(agreements[agreement_id], key, value)
    agreements[agreement_id].updated_at = datetime.utcnow()
    return agreements[agreement_id]


@app.post("/agreements/{agreement_id}/partners/add")
async def add_partner(agreement_id: str, partner: Partner):
    """Add a new partner to agreement."""
    if agreement_id not in agreements:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Agreement not found")
    agreements[agreement_id].partners.append(partner)
    agreements[agreement_id].capital_amount += partner.contribution
    agreements[agreement_id].updated_at = datetime.utcnow()
    return agreements[agreement_id]


@app.get("/agreements/{agreement_id}/summary")
async def get_agreement_summary(agreement_id: str):
    """Get partnership summary."""
    if agreement_id not in agreements:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Agreement not found")
    agg = agreements[agreement_id]
    return {
        "partnership_name": agg.partnership_name,
        "total_capital": agg.capital_amount,
        "partner_count": len(agg.partners),
        "partners": [{"name": p.name, "contribution": p.contribution, "profit_share": p.profit_sharing_ratio} for p in agg.partners]
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)