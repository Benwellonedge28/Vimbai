"""
Vimbai Dashboard Service
Financial dashboard data aggregation with KPI cards, charts, and widgets.
Port: 8365
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional, Any
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "dashboard-service"
PORT = int(os.getenv("PORT", "8365"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Dashboard Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class DashboardConfig(BaseModel):
    company_id: str; dashboard_type: str  # executive, financial, operations, compliance
    refresh_interval: int = 60
    period_start: str = ""; period_end: str = ""

class Widget(BaseModel):
    widget_id: str = Field(default_factory=lambda: str(uuid.uuid4())[:8])
    type: str  # kpi_card, chart, table, gauge, alert
    title: str; data: Dict[str, Any] = {}
    position: int = 0

class DashboardResponse(BaseModel):
    dashboard_id: str; company_id: str; dashboard_type: str
    widgets: List[Widget]; last_refreshed: str
    period_start: str = ""; period_end: str = ""

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

def _executive_widgets():
    return [
        Widget(type="kpi_card", title="Total Revenue", data={"value": 0, "format": "currency", "trend": "up"}, position=1),
        Widget(type="kpi_card", title="Net Income", data={"value": 0, "format": "currency", "trend": "down"}, position=2),
        Widget(type="kpi_card", title="Cash Position", data={"value": 0, "format": "currency"}, position=3),
        Widget(type="kpi_card", title="Profit Margin", data={"value": 0, "format": "percentage"}, position=4),
        Widget(type="chart", title="Revenue vs Expenses", data={"chart_type": "line", "periods": []}, position=5),
        Widget(type="chart", title="Cash Flow Trend", data={"chart_type": "bar"}, position=6),
    ]

def _financial_widgets():
    return [
        Widget(type="kpi_card", title="Current Ratio", data={"value": 0, "format": "decimal"}, position=1),
        Widget(type="kpi_card", title="Debt-to-Equity", data={"value": 0, "format": "decimal"}, position=2),
        Widget(type="kpi_card", title="ROE", data={"value": 0, "format": "percentage"}, position=3),
        Widget(type="chart", title="Balance Sheet", data={"chart_type": "stacked_bar"}, position=4),
        Widget(type="table", title="Top Accounts by Balance", data={"rows": []}, position=5),
    ]

def _operations_widgets():
    return [
        Widget(type="kpi_card", title="Inventory Turnover", data={"value": 0, "format": "decimal"}, position=1),
        Widget(type="kpi_card", title="Days Sales Outstanding", data={"value": 0, "format": "days"}, position=2),
        Widget(type="gauge", title="Capacity Utilization", data={"value": 0, "max": 100}, position=3),
        Widget(type="chart", title="Order Fulfillment", data={"chart_type": "area"}, position=4),
    ]

def _compliance_widgets():
    return [
        Widget(type="kpi_card", title="Compliance Score", data={"value": 0, "format": "percentage"}, position=1),
        Widget(type="alert", title="Pending Compliance Items", data={"alerts": [], "count": 0}, position=2),
        Widget(type="table", title="Audit Findings", data={"rows": []}, position=3),
    ]

@app.post("/load", response_model=DashboardResponse)
async def load_dashboard(req: DashboardConfig):
    widget_map = {
        "executive": _executive_widgets,
        "financial": _financial_widgets,
        "operations": _operations_widgets,
        "compliance": _compliance_widgets,
    }
    widgets = widget_map.get(req.dashboard_type, _executive_widgets)()
    
    return DashboardResponse(
        dashboard_id=f"DASH-{req.company_id}-{req.dashboard_type}-{datetime.now().strftime('%Y%m%d')}",
        company_id=req.company_id, dashboard_type=req.dashboard_type,
        widgets=widgets,
        last_refreshed=datetime.now(timezone.utc).isoformat(),
        period_start=req.period_start, period_end=req.period_end
    )

@app.post("/custom", response_model=DashboardResponse)
async def create_custom_dashboard(company_id: str, dashboard_type: str, widgets: List[Widget]):
    return DashboardResponse(
        dashboard_id=f"CUST-{company_id}-{uuid.uuid4().hex[:8]}",
        company_id=company_id, dashboard_type=dashboard_type,
        widgets=widgets,
        last_refreshed=datetime.now(timezone.utc).isoformat()
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
