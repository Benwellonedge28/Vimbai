# This file may be imported bare (Docker `uvicorn main:app`, bracket mounts), so it
# bootstraps its own package alias before importing sibling modules.
import importlib.util as _ilu
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
_PKG = "authorized_share_capital_service"
if _PKG not in _sys.modules or not hasattr(_sys.modules.get(_PKG), "__path__"):
    _spec = _ilu.spec_from_file_location(_PKG, _os.path.join(_HERE, "__init__.py"))
    _pkg = _ilu.module_from_spec(_spec)
    _pkg.__path__ = [_HERE]
    _sys.modules[_PKG] = _pkg

"""
Vimbai Authorized Share Capital Service
Manages authorized share capital, share classes, and issuance records.

Record-keeping only: this service records share capital movements
(user-owned, Book-scoped via X-User-Id / X-Book-ID); it never moves
money. Corrections use reversing entries.
"""

import os
from typing import List

import structlog
from authorized_share_capital_service import crud, models
from authorized_share_capital_service.database import Neo4jConnector
from authorized_share_capital_service.dependencies import book_id_var, get_db_session, get_user_id
from authorized_share_capital_service.exceptions import (
    ConflictError,
    NotFoundError,
    ValidationError,
)
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from neo4j import AsyncSession

SERVICE_NAME = "authorized-share-capital-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8017"))

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

app = FastAPI(title="Vimbai Authorized Share Capital Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Capture the Book context for the duration of the request."""
    token = book_id_var.set(request.headers.get("x-book-id"))
    try:
        return await call_next(request)
    finally:
        book_id_var.reset(token)


@app.on_event("startup")
async def startup():
    Neo4jConnector.configure(
        os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        os.getenv("NEO4J_USER", "neo4j"),
        os.getenv("NEO4J_PASSWORD", "password"),
    )


@app.on_event("shutdown")
async def shutdown():
    await Neo4jConnector.close_driver()


@app.exception_handler(NotFoundError)
async def not_found_exception_handler(request: Request, exc: NotFoundError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(ConflictError)
async def conflict_exception_handler(request: Request, exc: ConflictError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(ValidationError)
async def validation_exception_handler(request: Request, exc: ValidationError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/share-classes", response_model=models.ShareClass)
async def create_share_class(
    name: str,
    authorized_shares: int,
    par_value: float = 0.0,
    voting_rights: str = "ordinary",
    dividend_rate: float = 0.0,
    rights: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a share class."""
    valid_voting = ["ordinary", "preferential", "none"]
    if voting_rights not in valid_voting:
        raise HTTPException(status_code=400, detail=f"Invalid voting rights. Must be one of {valid_voting}")

    share_class = models.ShareClass(
        name=name,
        authorized_shares=authorized_shares,
        par_value=par_value,
        voting_rights=voting_rights,
        dividend_rate=dividend_rate,
        rights=rights,
    )
    created = await crud.create_share_class(db_session, user_id, share_class)
    logger.info("Share class created", class_id=created.id, name=name, authorized=authorized_shares)
    return created


@app.get("/share-classes", response_model=List[models.ShareClass])
async def list_share_classes(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all share classes."""
    return await crud.list_share_classes(db_session, user_id)


@app.post("/share-classes/{class_id}/issue", response_model=models.ShareIssuance)
async def issue_shares(
    class_id: str,
    number_of_shares: int,
    issue_price: float,
    issued_to: str = "",
    notes: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Issue shares from a share class."""
    share_class = await crud.get_share_class(db_session, user_id, class_id)
    if not share_class:
        raise HTTPException(status_code=404, detail="Share class not found")

    if share_class.issued_shares + number_of_shares > share_class.authorized_shares:
        raise HTTPException(status_code=400, detail="Issuance exceeds authorized shares")

    total_proceeds = number_of_shares * issue_price
    issuance = models.ShareIssuance(
        share_class_id=class_id,
        number_of_shares=number_of_shares,
        issue_price=issue_price,
        total_proceeds=total_proceeds,
        issued_to=issued_to,
        notes=notes,
    )
    share_class.issued_shares += number_of_shares
    await crud.save_share_class(db_session, user_id, share_class)
    created = await crud.create_issuance(db_session, user_id, issuance)
    logger.info("Shares issued", class_id=class_id, shares=number_of_shares, proceeds=total_proceeds)
    return created


@app.get("/share-classes/{class_id}/issuances", response_model=List[models.ShareIssuance])
async def list_issuances(
    class_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List share issuances for a class."""
    issuances = await crud.list_issuances(db_session, user_id)
    return [i for i in issuances if i.share_class_id == class_id]


@app.post("/share-classes/{class_id}/buyback", response_model=models.ShareBuyback)
async def buyback_shares(
    class_id: str,
    number_of_shares: int,
    buyback_price: float,
    notes: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Buy back shares from a share class."""
    share_class = await crud.get_share_class(db_session, user_id, class_id)
    if not share_class:
        raise HTTPException(status_code=404, detail="Share class not found")
    if number_of_shares > share_class.issued_shares:
        raise HTTPException(status_code=400, detail="Buyback exceeds issued shares")

    total_cost = number_of_shares * buyback_price
    buyback = models.ShareBuyback(
        share_class_id=class_id,
        number_of_shares=number_of_shares,
        buyback_price=buyback_price,
        total_cost=total_cost,
        notes=notes,
    )
    share_class.issued_shares -= number_of_shares
    await crud.save_share_class(db_session, user_id, share_class)
    created = await crud.create_buyback(db_session, user_id, buyback)
    logger.info("Shares bought back", class_id=class_id, shares=number_of_shares, cost=total_cost)
    return created


@app.get("/summary")
async def capital_summary(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get authorized share capital summary."""
    classes = await crud.list_share_classes(db_session, user_id)
    issuances = await crud.list_issuances(db_session, user_id)
    buybacks = await crud.list_buybacks(db_session, user_id)
    return {
        "total_classes": len(classes),
        "total_authorized": sum(sc.authorized_shares for sc in classes),
        "total_issued": sum(sc.issued_shares for sc in classes),
        "total_proceeds": sum(i.total_proceeds for i in issuances),
        "total_buyback_cost": sum(b.total_cost for b in buybacks),
        "by_class": [
            {
                "name": sc.name,
                "authorized": sc.authorized_shares,
                "issued": sc.issued_shares,
                "available": sc.authorized_shares - sc.issued_shares,
            }
            for sc in classes
        ],
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
