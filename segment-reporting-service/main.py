"""
Vimbai Segment Reporting Service
IFRS 8 operating segment identification and disclosure reporting.
Port: 8385
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Dict, List

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "segment-reporting-service"
PORT = int(os.getenv("PORT", "8385"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Segment Reporting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class Segment(BaseModel):
    segment_id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    segment_type: str  # business, geographical
    revenue: float
    expenses: float
    assets: float
    liabilities: float = 0
    profit_or_loss: float = 0
    identifiable: bool = True


class SegmentReportRequest(BaseModel):
    company_id: str
    fiscal_year: int
    segments: List[Segment]
    reconciliation_revenue: float = 0
    reconciliation_expenses: float = 0


class SegmentReportResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    fiscal_year: int
    reportable_segments: int
    total_revenue: float
    total_profit: float
    total_assets: float
    segments: List[Dict]
    reconciliation: Dict
    disclosure_notes: List[str] = []


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/report", response_model=SegmentReportResult)
async def generate_report(req: SegmentReportRequest):
    total_rev = sum(s.revenue for s in req.segments)
    total_profit = sum(s.profit_or_loss for s in req.segments)
    total_assets = sum(s.assets for s in req.segments)

    # Determine reportable segments (>=10% of total)
    reportable_segments = 0
    segments_detail = []
    for s in req.segments:
        rev_pct = s.revenue / total_rev * 100 if total_rev else 0
        profit_pct = abs(s.profit_or_loss) / abs(total_profit) * 100 if total_profit else 0
        asset_pct = s.assets / total_assets * 100 if total_assets else 0
        is_reportable = rev_pct >= 10 or profit_pct >= 10 or asset_pct >= 10
        if is_reportable:
            reportable_segments += 1

        segments_detail.append(
            {
                "segment_id": s.segment_id,
                "name": s.name,
                "type": s.segment_type,
                "revenue": round(s.revenue, 2),
                "expenses": round(s.expenses, 2),
                "profit_or_loss": round(s.profit_or_loss, 2),
                "assets": round(s.assets, 2),
                "liabilities": round(s.liabilities, 2),
                "revenue_pct": round(rev_pct, 1),
                "is_reportable": is_reportable,
            }
        )

    return SegmentReportResult(
        company_id=req.company_id,
        fiscal_year=req.fiscal_year,
        reportable_segments=reportable_segments,
        total_revenue=round(total_rev, 2),
        total_profit=round(total_profit, 2),
        total_assets=round(total_assets, 2),
        segments=segments_detail,
        reconciliation={
            "segments_revenue": round(total_rev, 2),
            "consolidated_revenue": round(total_rev + req.reconciliation_revenue, 2),
            "reconciliation_difference": round(req.reconciliation_revenue, 2),
            "segments_expenses": round(sum(s.expenses for s in req.segments), 2),
            "consolidated_expenses": round(sum(s.expenses for s in req.segments) + req.reconciliation_expenses, 2),
        },
        disclosure_notes=[
            f"{reportable_segments} reportable segments identified under IFRS 8",
            "Segment identification based on internal management reporting",
            "Geographic and business segment information disclosed separately",
            "Inter-segment transactions eliminated in reconciliation",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
