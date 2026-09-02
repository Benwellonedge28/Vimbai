"""
Vimbai Supply Chain Service
Inventory management, supplier tracking, order fulfillment, and demand forecasting.
Port: 8004
"""

import math
import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from typing import Dict, List, Optional

import structlog
from fastapi import FastAPI
from pydantic import BaseModel, Field

SERVICE_NAME = "supply-chain-service"
PORT = int(os.getenv("PORT", "8004"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ]
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Supply Chain Service", version="2.0.0", docs_url="/docs")
# Distributed tracing
try:
    from shared.tracing import setup_tracing

    setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass


class Supplier(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    contact: str = ""
    lead_time_days: int = 7
    rating: float = 5.0
    products: List[str] = []


class InventoryItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    sku: str
    name: str
    company_id: str
    quantity: int = 0
    reorder_point: int = 10
    reorder_qty: int = 50
    unit_cost: float = 0
    unit_price: float = 0
    supplier_id: Optional[str] = None
    lead_time_days: int = 7


class PurchaseOrder(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    supplier_id: str
    item_sku: str
    quantity: int
    unit_cost: float
    status: str = "pending"  # pending, approved, shipped, received
    order_date: str = Field(default_factory=lambda: datetime.now(timezone.utc).isoformat())
    expected_delivery: Optional[str] = None


class DemandForecast(BaseModel):
    sku: str
    company_id: str
    historical_data: List[float] = []  # units sold per period
    forecast_periods: int = 3


class ForecastResult(BaseModel):
    sku: str
    forecast: List[float]
    method: str
    confidence: float
    reorder_recommended: bool
    recommended_qty: int = 0


_inventory: Dict[str, List[InventoryItem]] = defaultdict(list)
_suppliers: Dict[str, Supplier] = {}
_orders: Dict[str, List[PurchaseOrder]] = defaultdict(list)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/suppliers", response_model=Supplier)
async def create_supplier(supplier: Supplier):
    _suppliers[supplier.id] = supplier
    return supplier


@app.get("/suppliers", response_model=List[Supplier])
async def list_suppliers():
    return list(_suppliers.values())


@app.post("/inventory", response_model=InventoryItem)
async def add_inventory(item: InventoryItem):
    _inventory[item.company_id].append(item)
    return item


@app.get("/inventory", response_model=List[InventoryItem])
async def get_inventory(company_id: str):
    return _inventory.get(company_id, [])


@app.get("/inventory/low-stock", response_model=List[Dict])
async def get_low_stock(company_id: str):
    items = _inventory.get(company_id, [])
    low = []
    for item in items:
        if item.quantity <= item.reorder_point:
            days_until_stockout = item.quantity / max(1, item.quantity) if item.quantity else 0
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


@app.post("/purchase-orders", response_model=PurchaseOrder)
async def create_po(po: PurchaseOrder):
    _orders[po.company_id].append(po)
    return po


@app.get("/purchase-orders", response_model=List[PurchaseOrder])
async def list_pos(company_id: str, status: str = ""):
    orders = _orders.get(company_id, [])
    if status:
        orders = [o for o in orders if o.status == status]
    return orders


@app.post("/purchase-orders/{po_id}/receive")
async def receive_po(po_id: str, company_id: str):
    orders = _orders.get(company_id, [])
    for po in orders:
        if po.id == po_id:
            po.status = "received"
            items = _inventory.get(company_id, [])
            for item in items:
                if item.sku == po.item_sku:
                    item.quantity += po.quantity
                    break
            return {"po_id": po_id, "status": "received", "quantity_added": po.quantity}
    from fastapi import HTTPException

    raise HTTPException(status_code=404, detail="PO not found")


@app.post("/forecast", response_model=ForecastResult)
async def forecast_demand(req: DemandForecast):
    if len(req.historical_data) < 2:
        return ForecastResult(
            sku=req.sku,
            forecast=[0] * req.forecast_periods,
            method="insufficient_data",
            confidence=0,
            reorder_recommended=False,
        )

    # Simple moving average with trend
    recent = req.historical_data[-min(5, len(req.historical_data)) :]
    avg = sum(recent) / len(recent)
    if len(recent) >= 2:
        trend = (recent[-1] - recent[0]) / len(recent)
    else:
        trend = 0

    forecast = [max(0, avg + trend * (i + 1)) for i in range(req.forecast_periods)]
    confidence = max(0, min(1, 1 - abs(trend) / (avg + 1)))

    # Check if reorder needed
    items = _inventory.get(req.company_id, [])
    item = next((i for i in items if i.sku == req.sku), None)
    reorder = False
    rec_qty = 0
    if item:
        projected_stock = item.quantity - sum(forecast)
        if projected_stock <= item.reorder_point:
            reorder = True
            rec_qty = item.reorder_qty

    return ForecastResult(
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
