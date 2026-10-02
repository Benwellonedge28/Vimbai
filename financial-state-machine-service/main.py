"""Vimbai Financial State Machine Service - document lifecycle states. Port: 8333

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "financial_state_machine_service" not in _sys.modules or not hasattr(
    _sys.modules.get("financial_state_machine_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "financial_state_machine_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["financial_state_machine_service"] = _pkg
    _sys.modules["financial_state_machine_service"].__path__ = [_HERE]

import os

import structlog
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from financial_state_machine_service import crud, models
from financial_state_machine_service.dependencies import book_id_var, get_db_session, get_user_id
from financial_state_machine_service.exceptions import FinancialStateMachineError
from financial_state_machine_service.models import TRANSITIONS, DocumentState
from neo4j import AsyncSession

SERVICE_NAME = "financial-state-machine-service"
PORT = int(os.getenv("PORT", "8333"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Financial State Machine Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="financial-state-machine-service", instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(FinancialStateMachineError)
async def _financial_state_machine_error(request: Request, exc: FinancialStateMachineError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/documents", response_model=models.FinancialDocument)
async def create_document(
    doc: models.FinancialDocument,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a document for the caller's Book (initial state: draft)."""
    return await crud.create_document(db_session, user_id, doc)


@app.get("/documents/{doc_id}")
async def get_document(
    doc_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Fetch a caller-owned, Book-visible document; cross-scope reads 404."""
    doc = await crud.find_document(db_session, user_id, doc_id)
    if doc is None:
        from fastapi import HTTPException

        raise HTTPException(status_code=404, detail="Document not found")
    return doc


@app.post("/documents/{doc_id}/transition")
async def transition(
    doc_id: str,
    to_state: DocumentState,
    user_id: str = "",
    notes: str = "",
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Apply a state transition (validated against the TRANSITIONS map); cross-scope 404.

    The user_id query param is the transition actor (kept from the original
    contract); the caller identity comes from the X-User-Id header.
    """
    result = await crud.apply_transition(db_session, caller_id, doc_id, to_state, actor_id=user_id, notes=notes)
    logger.info("state_transition", doc_id=doc_id, from_state=result["current_state"], to_state=to_state)
    return result


@app.get("/documents/{doc_id}/history")
async def get_history(
    doc_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Transition history for a caller-owned, Book-visible document."""
    doc = await crud.find_document(db_session, user_id, doc_id)
    if doc is None:
        from fastapi import HTTPException

        raise HTTPException(status_code=404, detail="Document not found")
    return {"doc_id": doc_id, "history": doc.history, "current_state": doc.current_state}


@app.get("/states")
async def get_states():
    """The state catalogue and allowed transitions (pure, no storage)."""
    return {
        "states": [s.value for s in DocumentState],
        "transitions": {k.value: [v.value for v in vs] for k, vs in TRANSITIONS.items()},
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
