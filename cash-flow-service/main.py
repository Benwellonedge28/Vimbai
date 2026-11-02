"""Vimbai Cash Flow Service - project cash flow schedules. Port: 8100

Schedules persist as :CashFlowSummary nodes, scoped to the caller and the Book;
the four calculators stay pure. This file may be imported bare (bracket mounts,
uvicorn main:app), so it bootstraps its own package alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "cash_flow_service" not in _sys.modules or not hasattr(_sys.modules.get("cash_flow_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("cash_flow_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["cash_flow_service"] = _pkg
    _sys.modules["cash_flow_service"].__path__ = [_HERE]

import os
from typing import List, Optional

import structlog
from cash_flow_service import crud, models
from cash_flow_service.dependencies import book_id_var, get_db_session, get_user_id
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Cash flow inflows and outflows"}


@app.post("/create", response_model=models.CashFlowSummary)
async def create_cash_flow(
    project_name: str,
    initial_investment: float,
    yearly_inflows: List[float],
    yearly_outflows: Optional[List[float]] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create cash flow schedule for a project."""
    summary = await crud.create_summary(
        db_session, user_id, project_name, initial_investment, yearly_inflows, yearly_outflows
    )
    logger.info("cash_flow_created", project_name=project_name, years=len(summary.entries), net=summary.total_net_flow)
    return summary


@app.post("/inflows")
async def calculate_inflows(inflows: List[float]):
    """Calculate total inflows."""
    return {"inflows": inflows, "total_inflows": sum(inflows)}


@app.post("/outflows")
async def calculate_outflows(outflows: List[float]):
    """Calculate total outflows."""
    return {"outflows": outflows, "total_outflows": sum(outflows)}


@app.post("/net-flows")
async def calculate_net_flows(inflows: List[float], outflows: List[float]):
    """Calculate net cash flows."""
    if len(outflows) < len(inflows):
        outflows = outflows + [0] * (len(inflows) - len(outflows))
    elif len(inflows) < len(outflows):
        inflows = inflows + [0] * (len(outflows) - len(inflows))

    net_flows = [i - o for i, o in zip(inflows, outflows)]
    return {"net_flows": net_flows, "total_net_flow": sum(net_flows)}


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
async def list_cash_flows(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's cash flow schedules."""
    return {"cash_flows": await crud.list_summaries(db_session, user_id)}


@app.get("/get/{cash_flow_id}")
async def get_cash_flow(
    cash_flow_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get specific cash flow by ID."""
    summary = await crud.get_summary(db_session, user_id, cash_flow_id)
    if summary is None:
        # the original answers 200 with this body rather than 404
        return {"error": "Cash flow not found"}
    return summary


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
