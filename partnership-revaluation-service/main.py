"""
FinAcc Partnership Revaluation Service
Asset revaluation and goodwill treatment in partnerships.
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

SERVICE_NAME = "partnership-revaluation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8044"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Partnership Revaluation Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class GoodwillTreatment(str, Enum):
    RAISE_AND_RAISE = "raise_and_raise"
    WRITE_OFF_AGAINST_RESERVES = "write_off_against_reserves"
    ELIMINATE_FROM_BOOKS = "eliminate_from_books"


class RevaluationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_id: str
    asset_name: str
    asset_code: str
    old_value: float
    new_value: float
    increase: float = 0
    decrease: float = 0
    revaluation_gain: float = 0
    revaluation_loss: float = 0
    journal_entry_id: Optional[str] = None


class RevaluationReport(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    partnership_id: str
    revaluation_date: datetime
    entries: List[RevaluationEntry] = []
    total_increase: float = 0
    total_decrease: float = 0
    net_gain: float = 0
    goodwill_amount: float = 0
    goodwill_treatment: GoodwillTreatment
    new_profit_sharing_ratios: Dict[str, float] = {}
    journal_entry_ids: List[str] = []
    created_at: datetime = Field(default_factory=datetime.utcnow)


revaluations: List[RevaluationReport] = []


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
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Partnership asset revaluation"}


@app.post("/revalue")
async def create_revaluation(
    partnership_id: str, revaluation_date: datetime, goodwill_treatment: GoodwillTreatment,
    asset_revaluations: List[Dict[str, Any]], goodwill_amount: float = 0
):
    """Create asset revaluation."""
    entries = []
    total_increase = 0
    total_decrease = 0
    journal_entries = []

    for rev in asset_revaluations:
        entry = RevaluationEntry(
            asset_id=rev["asset_id"], asset_name=rev["asset_name"], asset_code=rev["asset_code"],
            old_value=rev["old_value"], new_value=rev["new_value"]
        )
        if entry.new_value > entry.old_value:
            entry.increase = entry.new_value - entry.old_value
            entry.revaluation_gain = entry.increase
            total_increase += entry.increase
            journal_entries.append({
                "date": revaluation_date,
                "description": f"Revaluation gain - {entry.asset_name}",
                "entries": [
                    {"account_code": rev["account_code"], "description": entry.asset_name, "debit": entry.increase, "credit": 0},
                    {"account_code": "3100", "description": "Revaluation Reserve", "debit": 0, "credit": entry.increase},
                ],
                "reference": f"REV-{entry.id[:8]}"
            })
        else:
            entry.decrease = entry.old_value - entry.new_value
            entry.revaluation_loss = entry.decrease
            total_decrease += entry.decrease
            journal_entries.append({
                "date": revaluation_date,
                "description": f"Revaluation loss - {entry.asset_name}",
                "entries": [
                    {"account_code": "6200", "description": "Revaluation Loss", "debit": entry.decrease, "credit": 0},
                    {"account_code": rev["account_code"], "description": entry.asset_name, "debit": 0, "credit": entry.decrease},
                ],
                "reference": f"REV-{entry.id[:8]}"
            })
        entries.append(entry)

    # Goodwill treatment
    if goodwill_amount > 0 and goodwill_treatment == GoodwillTreatment.RAISE_AND_RAISE:
        journal_entries.append({
            "date": revaluation_date,
            "description": "Goodwill arising on revaluation",
            "entries": [
                {"account_code": "1500", "description": "Goodwill", "debit": goodwill_amount, "credit": 0},
                {"account_code": "3100", "description": "Revaluation Reserve", "debit": 0, "credit": goodwill_amount},
            ],
            "reference": f"GW-REV-{revaluation_date.strftime('%Y%m')}"
        })

    # Post all journal entries
    entry_ids = []
    for entry in journal_entries:
        result = await call_accounting_service("POST", "/journal-entries", entry)
        entry_ids.append(result.get("id", ""))
        for reventry in entries:
            if entry["reference"].endswith(reventry.id[:8]):
                reventry.journal_entry_id = result.get("id")

    report = RevaluationReport(
        partnership_id=partnership_id, revaluation_date=revaluation_date,
        entries=entries, total_increase=total_increase, total_decrease=total_decrease,
        net_gain=total_increase - total_decrease, goodwill_amount=goodwill_amount,
        goodwill_treatment=goodwill_treatment, journal_entry_ids=entry_ids
    )
    revaluations.append(report)
    return report


@app.get("/revaluations")
async def list_revaluations(partnership_id: Optional[str] = None):
    """List revaluations."""
    result = revaluations
    if partnership_id:
        result = [r for r in result if r.partnership_id == partnership_id]
    return {"revaluations": result, "count": len(result)}


@app.get("/revaluations/{revaluation_id}")
async def get_revaluation(revaluation_id: str):
    """Get revaluation details."""
    rev = next((r for r in revaluations if r.id == revaluation_id), None)
    if not rev:
        return {"error": "Revaluation not found"}
    return rev


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)