"""
Vimbai Intercompany Service
Manages intercompany transactions, transfer pricing, and eliminations.
Caller-owned (X-User-Id) and Book-gated (X-Book-ID) stores persist in Neo4j.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import logging
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "intercompany_service" not in _sys.modules or not hasattr(_sys.modules.get("intercompany_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("intercompany_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["intercompany_service"] = _pkg
    _sys.modules["intercompany_service"].__path__ = [_HERE]

from typing import List, Optional

from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from intercompany_service import crud
from intercompany_service.dependencies import book_id_var, get_db_session, get_user_id
from intercompany_service.models import EliminationEntry, IntercompanyEntity, IntercompanyTransaction
from neo4j import AsyncSession

SERVICE_NAME = "intercompany-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8350"))

try:
    import structlog

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
except ImportError:  # pragma: no cover
    logging.basicConfig(level=logging.INFO)
    logger = logging.getLogger(SERVICE_NAME)

app = FastAPI(title="Vimbai Intercompany Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/entities", response_model=IntercompanyEntity)
async def create_entity(
    name: str,
    legal_entity_code: str,
    tax_jurisdiction: str = "",
    currency: str = "USD",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Register an intercompany entity."""
    entity = IntercompanyEntity(
        name=name,
        legal_entity_code=legal_entity_code,
        tax_jurisdiction=tax_jurisdiction,
        currency=currency,
    )
    saved = await crud.create_entity(db_session, user_id, entity)
    logger.info("Intercompany entity created", entity_id=saved.id, name=name)
    return saved


@app.get("/entities", response_model=List[IntercompanyEntity])
async def list_entities(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all intercompany entities (caller's own Book-visible set)."""
    return await crud.list_entities(db_session, user_id)


@app.post("/transactions", response_model=IntercompanyTransaction)
async def create_transaction(
    from_entity_id: str,
    to_entity_id: str,
    transaction_type: str,
    amount: float,
    currency: str = "USD",
    description: str = "",
    transfer_price_basis: str = "cost_plus",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create an intercompany transaction."""
    valid_types = ["loan", "service_fee", "royalty", "sale", "cost_allocation"]
    if transaction_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid type. Must be one of {valid_types}")

    txn = IntercompanyTransaction(
        from_entity_id=from_entity_id,
        to_entity_id=to_entity_id,
        transaction_type=transaction_type,
        amount=amount,
        currency=currency,
        description=description,
        transfer_price_basis=transfer_price_basis,
    )
    saved = await crud.create_transaction(db_session, user_id, txn)
    logger.info("Intercompany transaction created", txn_id=saved.id, amount=amount)
    return saved


@app.get("/transactions", response_model=List[IntercompanyTransaction])
async def list_transactions(
    from_entity: Optional[str] = None,
    to_entity: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List intercompany transactions (caller's own Book-visible set)."""
    result = await crud.list_transactions(db_session, user_id)
    if from_entity:
        result = [t for t in result if t.from_entity_id == from_entity]
    if to_entity:
        result = [t for t in result if t.to_entity_id == to_entity]
    if status:
        result = [t for t in result if t.status == status]
    return result


@app.post("/transactions/match")
async def match_transactions(
    txn1_id: str,
    txn2_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Match two intercompany transactions for elimination (caller's own pairs only)."""
    txn1 = await crud.get_transaction(db_session, user_id, txn1_id)
    txn2 = await crud.get_transaction(db_session, user_id, txn2_id)
    if not txn1 or not txn2:
        raise HTTPException(status_code=404, detail="Transaction not found")

    if txn1.amount != txn2.amount:
        raise HTTPException(status_code=400, detail="Amounts do not match")

    await crud.mark_matched(db_session, user_id, txn1_id, txn2_id)
    await crud.mark_matched(db_session, user_id, txn2_id, txn1_id)

    elimination = EliminationEntry(
        pair_id=f"{txn1_id}:{txn2_id}",
        debit_entity_id=txn1.from_entity_id,
        credit_entity_id=txn2.from_entity_id,
        amount=txn1.amount,
        description=f"Elimination: {txn1.description}",
    )
    saved = await crud.create_elimination(db_session, user_id, elimination)

    logger.info("Transactions matched and eliminated", txn1=txn1_id, txn2=txn2_id, amount=txn1.amount)
    return {"matched": True, "elimination_id": saved.id}


@app.get("/eliminations", response_model=List[EliminationEntry])
async def list_eliminations(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List elimination entries (caller's own Book-visible set)."""
    return await crud.list_eliminations(db_session, user_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
