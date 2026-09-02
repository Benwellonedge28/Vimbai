"""
Journal Entries Service
Port: 8331
Journal entry management and validation
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Journal Entries Service", version="1.0.0")


class JournalEntry(BaseModel):
    entry_id: str
    entry_date: str
    description: str
    debit_account: str
    credit_account: str
    amount: float
    source_document: str
    status: str
    posted_by: str


class JournalEntriesRequest(BaseModel):
    company_id: str
    entries: List[JournalEntry]
    period_start: str
    period_end: str


class JournalEntriesResponse(BaseModel):
    company_id: str
    entries_summary: Dict[str, Any]
    entries_by_type: Dict[str, Any]
    pending_entries: List[Dict[str, Any]]
    validation_results: Dict[str, Any]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "journal-entries", "version": "1.0.0"}


@app.post("/analyze", response_model=JournalEntriesResponse)
async def analyze_journal_entries(request: JournalEntriesRequest):
    logger.info("Analyzing journal entries", company=request.company_id)

    total_entries = len(request.entries)
    posted_entries = sum(1 for e in request.entries if e.status == "Posted")
    pending_entries_list = [e for e in request.entries if e.status == "Pending"]
    rejected_entries = sum(1 for e in request.entries if e.status == "Rejected")

    total_debits = sum(e.amount for e in request.entries)
    total_credits = sum(e.amount for e in request.entries)

    entries_summary = {
        "total_entries": total_entries,
        "posted": posted_entries,
        "pending": len(pending_entries_list),
        "rejected": rejected_entries,
        "posting_rate": round(posted_entries / total_entries * 100, 2) if total_entries else 0,
    }

    entries_by_type = {
        "standard_entries": sum(1 for e in request.entries if "Standard" in e.description),
        "adjusting_entries": sum(1 for e in request.entries if "Adjusting" in e.description),
        "closing_entries": sum(1 for e in request.entries if "Closing" in e.description),
        "reversing_entries": sum(1 for e in request.entries if "Reversing" in e.description),
    }

    pending_entries = [
        {"entry_id": e.entry_id, "description": e.description, "amount": e.amount, "date": e.entry_date}
        for e in pending_entries_list[:10]
    ]

    validation_results = {
        "debits_equals_credits": abs(total_debits - total_credits) < 0.01,
        "total_debits": round(total_debits, 2),
        "total_credits": round(total_credits, 2),
        "variance": round(abs(total_debits - total_credits), 2),
    }

    recommendations = []
    if len(pending_entries_list) > 20:
        recommendations.append(f"{len(pending_entries_list)} pending entries - expedite posting")
    if rejected_entries > total_entries * 0.05:
        recommendations.append("High rejection rate - review entry procedures")

    return JournalEntriesResponse(
        company_id=request.company_id,
        entries_summary=entries_summary,
        entries_by_type=entries_by_type,
        pending_entries=pending_entries,
        validation_results=validation_results,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8331)
