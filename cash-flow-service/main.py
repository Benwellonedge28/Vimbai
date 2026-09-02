"""
Vimbai Cash Flow Service
Handles cash flow inflows and outflows for investment appraisal.
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

SERVICE_NAME = "cash-flow-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8100"))

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

app = FastAPI(title="Vimbai Cash Flow Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


class CashFlowEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    year: int
    inflow: float = 0
    outflow: float = 0
    net_flow: float = 0
    description: str = ""


class CashFlowSummary(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    project_name: str
    initial_investment: float
    entries: List[CashFlowEntry]
    total_inflows: float = 0
    total_outflows: float = 0
    total_net_flow: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


cash_flows: List[CashFlowSummary] = []


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Cash flow inflows and outflows"}


@app.post("/create")
async def create_cash_flow(
    project_name: str,
    initial_investment: float,
    yearly_inflows: List[float],
    yearly_outflows: Optional[List[float]] = None,
):
    """Create cash flow schedule for a project."""
    entries = []
    for i, inflow in enumerate(yearly_inflows):
        outflow = yearly_outflows[i] if yearly_outflows and i < len(yearly_outflows) else 0
        net_flow = inflow - outflow
        entries.append(CashFlowEntry(year=i + 1, inflow=inflow, outflow=outflow, net_flow=net_flow))

    total_inflows = sum(e.inflow for e in entries)
    total_outflows = sum(e.outflow for e in entries)
    total_net_flow = sum(e.net_flow for e in entries)

    summary = CashFlowSummary(
        project_name=project_name,
        initial_investment=initial_investment,
        entries=entries,
        total_inflows=total_inflows,
        total_outflows=total_outflows,
        total_net_flow=total_net_flow,
    )
    cash_flows.append(summary)
    return summary


@app.post("/inflows")
async def calculate_inflows(inflows: List[float]):
    """Calculate total inflows."""
    total = sum(inflows)
    return {"inflows": inflows, "total_inflows": total}


@app.post("/outflows")
async def calculate_outflows(outflows: List[float]):
    """Calculate total outflows."""
    total = sum(outflows)
    return {"outflows": outflows, "total_outflows": total}


@app.post("/net-flows")
async def calculate_net_flows(inflows: List[float], outflows: List[float]):
    """Calculate net cash flows."""
    if len(outflows) < len(inflows):
        outflows = outflows + [0] * (len(inflows) - len(outflows))
    elif len(inflows) < len(outflows):
        inflows = inflows + [0] * (len(outflows) - len(inflows))

    net_flows = [i - o for i, o in zip(inflows, outflows)]
    total_net = sum(net_flows)
    return {"net_flows": net_flows, "total_net_flow": total_net}


@app.post("/cumulative-flows")
async def calculate_cumulative_flows(net_flows: List[float]):
    """Calculate cumulative cash flows."""
    cumulative = []
    running_total = 0
    for flow in net_flows:
        running_total += flow
        cumulative.append(running_total)
    return {"net_flows": net_flows, "cumulative_flows": cumulative}


@app.get("/list")
async def list_cash_flows():
    """List all cash flow summaries."""
    return {"cash_flows": cash_flows}


@app.get("/get/{cash_flow_id}")
async def get_cash_flow(cash_flow_id: str):
    """Get specific cash flow by ID."""
    for cf in cash_flows:
        if cf.id == cash_flow_id:
            return cf
    return {"error": "Cash flow not found"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
