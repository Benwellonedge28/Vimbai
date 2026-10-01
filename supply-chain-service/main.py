"""Vimbai Supply Chain Service - inventory, suppliers, purchase orders and demand forecasting. Port: 8004

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "supply_chain_service" not in _sys.modules or not hasattr(_sys.modules.get("supply_chain_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("supply_chain_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["supply_chain_service"] = _pkg
    _sys.modules["supply_chain_service"].__path__ = [_HERE]

import os
from typing import Dict, List

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from supply_chain_service import crud, models
from supply_chain_service.dependencies import book_id_var, get_db_session, get_user_id
from supply_chain_service.exceptions import SupplyChainError

SERVICE_NAME = "supply-chain-service"
PORT = int(os.getenv("PORT", "8004"))
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
app = FastAPI(title="Vimbai Supply Chain Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(SupplyChainError)
async def _supply_chain_error(request: Request, exc: SupplyChainError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/suppliers", response_model=models.Supplier)
async def create_supplier(
    supplier: models.Supplier,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_supplier(db_session, user_id, supplier)
    logger.info("supplier_created", supplier_id=item.id, name=item.name)
    return item


@app.get("/suppliers", response_model=List[models.Supplier])
async def list_suppliers(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_suppliers(db_session, user_id)


@app.post("/inventory", response_model=models.InventoryItem)
async def add_inventory(
    item: models.InventoryItem,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    created = await crud.add_inventory(db_session, user_id, item)
    logger.info("inventory_added", sku=created.sku, company_id=created.company_id)
    return created


@app.get("/inventory", response_model=List[models.InventoryItem])
async def get_inventory(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_inventory(db_session, user_id, company_id)


@app.get("/inventory/low-stock", response_model=List[Dict])
async def get_low_stock(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    items = await crud.list_inventory(db_session, user_id, company_id)
    low = []
    for item in items:
        if item.quantity <= item.reorder_point:
            low.append(
                {
                    "sku": item.sku,
                    "name": item.name,
                    "quantity": item.quantity,
                    "reorder_point": item.reorder_point,
                    "reorder_qty": item.reorder_qty,
                    "supplier_id": item.supplier_id,
                    "urgency": "critical" if item.quantity == 0 else "warning",
                }
            )
    return low


@app.post("/purchase-orders", response_model=models.PurchaseOrder)
async def create_po(
    po: models.PurchaseOrder,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    created = await crud.create_po(db_session, user_id, po)
    logger.info("po_created", po_id=created.id, company_id=created.company_id)
    return created


@app.get("/purchase-orders", response_model=List[models.PurchaseOrder])
async def list_pos(
    company_id: str,
    status: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_pos(db_session, user_id, company_id, status)


@app.post("/purchase-orders/{po_id}/receive")
async def receive_po(
    po_id: str,
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    try:
        result = await crud.receive_po(db_session, user_id, company_id, po_id)
        logger.info("po_received", po_id=po_id, company_id=company_id)
        return result
    except SupplyChainError as exc:
        raise HTTPException(status_code=getattr(exc, "status_code", 404), detail=str(exc))


@app.post("/forecast", response_model=models.ForecastResult)
async def forecast_demand(
    req: models.DemandForecast,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Moving average with trend; the reorder check reads the caller's Book-visible inventory."""
    if len(req.historical_data) < 2:
        return models.ForecastResult(
            sku=req.sku,
            forecast=[0] * req.forecast_periods,
            method="insufficient_data",
            confidence=0,
            reorder_recommended=False,
        )

    recent = req.historical_data[-min(5, len(req.historical_data)) :]
    avg = sum(recent) / len(recent)
    if len(recent) >= 2:
        trend = (recent[-1] - recent[0]) / len(recent)
    else:
        trend = 0

    forecast = [max(0, avg + trend * (i + 1)) for i in range(req.forecast_periods)]
    confidence = max(0, min(1, 1 - abs(trend) / (avg + 1)))

    # Check if reorder needed (caller's Book-visible inventory only)
    items = await crud.list_inventory(db_session, user_id, req.company_id)
    item = next((i for i in items if i.sku == req.sku), None)
    reorder = False
    rec_qty = 0
    if item:
        projected_stock = item.quantity - sum(forecast)
        if projected_stock <= item.reorder_point:
            reorder = True
            rec_qty = item.reorder_qty

    return models.ForecastResult(
        sku=req.sku,
        forecast=[round(f, 1) for f in forecast],
        method="moving_average_with_trend",
        confidence=round(confidence, 2),
        reorder_recommended=reorder,
        recommended_qty=rec_qty,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
