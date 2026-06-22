"""
Events After Reporting Service
Port: 8206
Post-balance sheet events under IAS 10
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Events After Reporting Service", version="1.0.0")

class EventItem(BaseModel):
    event_id: str
    event_date: str
    description: str
    event_type: str
    adjusting: bool
    financial_impact: float

class EventsAfterReportingRequest(BaseModel):
    company_id: str
    audit_id: str
    balance_sheet_date: str
    financial_statements_date: str
    authorization_date: str
    events: List[Dict[str, Any]]
    litigation_updates: List[Dict[str, Any]]

class EventsAfterReportingResponse(BaseModel):
    company_id: str
    audit_id: str
    balance_sheet_date: str
    authorization_date: str
    adjusting_events: int
    non_adjusting_events: int
    events_identified: List[EventItem]
    total_adjusting_impact: float
    significant_non_adjusting: List[str]
    disclosure_required: bool
    recommendations: List[str]

async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "events-after-reporting", "version": "1.0.0"}

@app.post("/analyze", response_model=EventsAfterReportingResponse)
async def analyze_events_after_reporting(request: EventsAfterReportingRequest):
    logger.info("Analyzing events after reporting", company=request.company_id, audit=request.audit_id)

    events_identified = []
    adjusting_impact = 0.0
    adjusting_count = 0
    non_adjusting_count = 0
    significant_non_adjusting = []

    for event in request.events:
        adjusting = event.get("adjusting_event", False)
        impact = event.get("financial_impact", 0.0)

        if adjusting:
            adjusting_count += 1
            adjusting_impact += abs(impact)
        else:
            non_adjusting_count += 1
            if abs(impact) > 5000000:
                significant_non_adjusting.append(event.get("description", ""))

    for lit in request.litigation_updates:
        events_identified.append(EventItem(
            event_id=lit.get("id", ""),
            event_date=lit.get("settlement_date", ""),
            description=f"Litigation settlement: {lit.get('description', '')}",
            event_type="legal",
            adjusting=lit.get("adjusting", False),
            financial_impact=lit.get("amount", 0.0)
        ))

    disclosure_required = adjusting_count > 0 or non_adjusting_count > 0

    return EventsAfterReportingResponse(
        company_id=request.company_id,
        audit_id=request.audit_id,
        balance_sheet_date=request.balance_sheet_date,
        authorization_date=request.authorization_date,
        adjusting_events=adjusting_count,
        non_adjusting_events=non_adjusting_count,
        events_identified=events_identified,
        total_adjusting_impact=round(adjusting_impact, 2),
        significant_non_adjusting=significant_non_adjusting if significant_non_adjusting else ["No significant non-adjusting events"],
        disclosure_required=disclosure_required,
        recommendations=[
            "Adjust financial statements for adjusting events",
            "Disclose significant non-adjusting events in notes",
            "Update going concern assessment if applicable",
            "Review subsequent events up to report authorization date"
        ]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8206)
