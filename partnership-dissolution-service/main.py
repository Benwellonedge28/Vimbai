"""
Vimbai Partnership Dissolution Service
Handles complete dissolution of partnerships.

Dissolution reports persist in Neo4j, stamped with the caller
(X-User-Id) and the Book context (X-Book-ID, verified upstream by the
API gateway). Listings and lookups are scoped to the caller's own
Book-visible records. Realization/settlement math, the {"dissolutions": ...}
list shape, and the {"error": "Not found"} lookup miss response are
preserved exactly.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
from datetime import datetime
from typing import Any, Dict, List

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "partnership_dissolution_service" not in _sys.modules or not hasattr(
    _sys.modules.get("partnership_dissolution_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "partnership_dissolution_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["partnership_dissolution_service"] = _pkg
    _sys.modules["partnership_dissolution_service"].__path__ = [_HERE]

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from partnership_dissolution_service import crud
from partnership_dissolution_service.database import Neo4jConnector
from partnership_dissolution_service.dependencies import book_id_var, get_db_session, get_user_id
from partnership_dissolution_service.exceptions import PartnershipDissolutionServiceError
from partnership_dissolution_service.models import (
    AssetRealization,
    CreditorSettlement,
    DissolutionReason,
    DissolutionReport,
    PartnerSettlement,
)

SERVICE_NAME = "partnership-dissolution-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8045"))

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

app = FastAPI(title="Vimbai Partnership Dissolution Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(PartnershipDissolutionServiceError)
async def _pd_error(request: Request, exc: PartnershipDissolutionServiceError):
    from fastapi.responses import JSONResponse

    return JSONResponse(
        status_code=getattr(exc, "status_code", 400),
        content={"detail": str(exc), "error": exc.__class__.__name__},
    )


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "description": "Partnership dissolution service"}


@app.post("/dissolve")
async def create_dissolution(
    partnership_id: str,
    dissolution_date: datetime,
    reason: DissolutionReason,
    assets: List[Dict[str, Any]],
    creditors: List[Dict[str, Any]],
    partners: List[Dict[str, Any]],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Process partnership dissolution."""
    report = DissolutionReport(partnership_id=partnership_id, dissolution_date=dissolution_date, reason=reason)

    # Asset realizations
    for asset in assets:
        realization = AssetRealization(
            asset_id=asset["asset_id"],
            asset_name=asset["asset_name"],
            book_value=asset["book_value"],
            sale_proceeds=asset["sale_proceeds"],
        )
        if realization.sale_proceeds > realization.book_value:
            realization.profit = realization.sale_proceeds - realization.book_value
            report.realization_profit += realization.profit
        else:
            realization.loss = realization.book_value - realization.sale_proceeds
            report.realization_loss += realization.loss
        report.assets.append(realization)
        report.total_assets_realized += realization.sale_proceeds

    # Creditor settlements
    for creditor in creditors:
        settlement = CreditorSettlement(
            creditor_id=creditor["id"],
            creditor_name=creditor["name"],
            amount_owed=creditor["amount"],
            amount_paid=creditor.get("paid", creditor["amount"]),
            discount_received=creditor.get("discount", 0),
        )
        report.creditors.append(settlement)
        report.total_creditors += settlement.amount_paid

    # Partner settlements
    for partner in partners:
        settlement = PartnerSettlement(
            partner_id=partner["id"],
            partner_name=partner["name"],
            capital_balance=partner["capital"],
            current_account_balance=partner.get("current", 0),
            share_of_profit_loss=partner.get("share", 0),
            total_due=0,
        )
        settlement.total_due = (
            settlement.capital_balance + settlement.current_account_balance + settlement.share_of_profit_loss
        )
        report.partners.append(settlement)
        report.total_partners_capitals += settlement.total_due

    report.status = "completed"
    return await crud.create_dissolution(db_session, user_id, report)


@app.get("/dissolutions")
async def list_dissolutions(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return {"dissolutions": await crud.list_dissolutions(db_session, user_id)}


@app.get("/dissolutions/{dissolution_id}")
async def get_dissolution(
    dissolution_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    report = await crud.get_dissolution(db_session, user_id, dissolution_id)
    return report if report else {"error": "Not found"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
