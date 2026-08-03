"""
Vimbai Authorized Share Capital Service
Manages authorized share capital and company structure.
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

SERVICE_NAME = "authorized-share-capital-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8047"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Authorized Share Capital Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class ShareClass(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    class_name: str  # Ordinary, Preference, etc.
    nominal_value: float
    authorized_quantity: int
    rights: str = ""


class CompanyCapital(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    company_name: str
    incorporation_date: datetime
    share_classes: List[ShareClass] = []
    total_authorized_capital: float = 0
    memorandum_document: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


capitals: Dict[str, CompanyCapital] = {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Authorized share capital management"}


@app.post("/register")
async def register_capital(data: CompanyCapital):
    """Register authorized share capital."""
    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()
    data.total_authorized_capital = sum(
        sc.nominal_value * sc.authorized_quantity for sc in data.share_classes
    )
    capitals[data.id] = data
    return data


@app.get("/companies/{company_id}")
async def get_company_capital(company_id: str):
    """Get company capital structure."""
    return next((c for c in capitals.values() if c.company_id == company_id), {"error": "Not found"})


@app.get("/companies")
async def list_companies():
    return {"companies": list(capitals.values())}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)