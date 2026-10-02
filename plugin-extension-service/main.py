"""Vimbai Plugin Extension Service. Port: 8390

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "plugin_extension_service" not in _sys.modules or not hasattr(
    _sys.modules.get("plugin_extension_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("plugin_extension_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["plugin_extension_service"] = _pkg
    _sys.modules["plugin_extension_service"].__path__ = [_HERE]

import time
from typing import Optional

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from plugin_extension_service import crud, models
from plugin_extension_service.dependencies import book_id_var, get_db_session, get_user_id
from plugin_extension_service.exceptions import PluginExtensionError

SERVICE_NAME = "plugin-extension-service"
PORT = int(__import__("os").getenv("PORT", "8390"))
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
app = FastAPI(title="Vimbai Plugin Extension Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="plugin-extension-service", instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(PluginExtensionError)
async def _disaster_recovery_error(request: Request, exc: PluginExtensionError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0", "uptime_seconds": time.time()}


@app.post("/items")
async def create_item(
    company_id: str,
    item: models.Entity,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    created = await crud.create_item(db_session, user_id, company_id, item)
    logger.info("item_created", company_id=company_id, name=created.name)
    return {"id": created.id, "name": created.name, "status": "created"}


@app.get("/items/{company_id}")
async def get_items(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    items = await crud.list_items_for_company(db_session, user_id, company_id)
    return {"company_id": company_id, "items": items, "total": len(items)}


@app.put("/items/{item_id}")
async def update_item(
    item_id: str,
    name: Optional[str] = None,
    description: Optional[str] = None,
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update a caller-owned, Book-visible item; cross-scope updates 404."""
    await crud.update_item(db_session, user_id, item_id, name, description, status)
    return {"id": item_id, "status": "updated"}


@app.delete("/items/{item_id}")
async def delete_item(
    item_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Soft delete (original semantics): flip status to 'deleted'; the item stays listed."""
    await crud.delete_item(db_session, user_id, item_id)
    return {"id": item_id, "status": "deleted"}


@app.get("/metrics")
async def metrics(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Service totals over the caller's Book-visible items only."""
    m = await crud.metrics(db_session, user_id)
    return {"service": SERVICE_NAME, **m}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
