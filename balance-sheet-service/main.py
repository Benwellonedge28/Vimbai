"""Vimbai Balance Sheet Service - Generate and manage balance sheets. Port: 8345"""
import os, uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "balance-sheet-service"
PORT = int(os.getenv("PORT", "8345"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Balance Sheet Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="balance-sheet-service", instrument_app=app)
except ImportError:
    TRACER = None

class AssetItem(BaseModel):
    name: str
    amount: float
    category: str = "current"  # current, non_current
    is_liquid: bool = False

class LiabilityItem(BaseModel):
    name: str
    amount: float
    category: str = "current"  # current, non_current
    due_date: Optional[datetime] = None

class EquityItem(BaseModel):
    name: str
    amount: float

class BalanceSheet(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    as_of_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    assets: List[AssetItem] = []
    liabilities: List[LiabilityItem] = []
    equity: List[EquityItem] = []
    total_assets: float = 0
    total_liabilities: float = 0
    total_equity: float = 0
    is_balanced: bool = False

_sheets: Dict[str, List[BalanceSheet]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/generate", response_model=BalanceSheet)
async def generate_balance_sheet(sheet: BalanceSheet):
    sheet.total_assets = sum(a.amount for a in sheet.assets)
    sheet.total_liabilities = sum(l.amount for l in sheet.liabilities)
    sheet.total_equity = sum(e.amount for e in sheet.equity)
    sheet.is_balanced = abs(sheet.total_assets - (sheet.total_liabilities + sheet.total_equity)) < 0.01
    _sheets[sheet.company_id].append(sheet)
    if not sheet.is_balanced:
        logger.warning("balance_sheet_unbalanced", company_id=sheet.company_id, diff=sheet.total_assets - (sheet.total_liabilities + sheet.total_equity))
    return sheet

@app.get("/latest/{company_id}")
async def get_latest(company_id: str):
    sheets = _sheets.get(company_id, [])
    if not sheets:
        raise HTTPException(status_code=404, detail="No balance sheets found")
    return sheets[-1]

@app.get("/history/{company_id}")
async def get_history(company_id: str):
    return {"company_id": company_id, "sheets": _sheets.get(company_id, []), "total": len(_sheets.get(company_id, []))}

@app.get("/ratios/{company_id}")
async def get_ratios(company_id: str):
    sheets = _sheets.get(company_id, [])
    if not sheets:
        raise HTTPException(status_code=404, detail="No balance sheets found")
    s = sheets[-1]
    current_assets = sum(a.amount for a in s.assets if a.category == "current")
    current_liabilities = sum(l.amount for l in s.liabilities if l.category == "current")
    liquid_assets = sum(a.amount for a in s.assets if a.is_liquid)
    return {
        "current_ratio": current_assets / max(1, current_liabilities),
        "quick_ratio": liquid_assets / max(1, current_liabilities),
        "debt_to_equity": s.total_liabilities / max(1, s.total_equity),
        "debt_to_assets": s.total_liabilities / max(1, s.total_assets),
        "equity_ratio": s.total_equity / max(1, s.total_assets),
    }

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
