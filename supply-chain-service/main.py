from fastapi import FastAPI, Depends, HTTPException, status, Query
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from supply_chain_service import models, crud
from supply_chain_service.database import init_db_schema, Neo4jConnector
from supply_chain_service.dependencies import get_db_session, get_user_id
from supply_chain_service.utils.auth import check_permission
from supply_chain_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError
import os
from dotenv import load_dotenv
from datetime import datetime
from pydantic import ValidationError as PydanticValidationError

# Load environment variables
load_dotenv()

app = FastAPI(
    title="FinAcc Supply Chain Service", # Renamed
    description="Manages customer invoicing, suppliers, inventory, and purchase orders.", # Updated description
    version="0.1.0",
)

@app.on_event("startup")
async def startup_event():
    Neo4jConnector.configure(
        uri=os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        user=os.getenv("NEO4J_USER", "neo4j"),
        password=os.getenv("NEO4J_PASSWORD", "neo4j")
    )
    Neo4jConnector.get_driver()
    await init_db_schema()

@app.on_event("shutdown")
async def shutdown_event():
    await Neo4jConnector.close_driver()

# --- Global Exception Handlers ---
@app.exception_handler(NotFoundError)
async def not_found_exception_handler(request, exc: NotFoundError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )

@app.exception_handler(ConflictError)
async def conflict_exception_handler(request, exc: ConflictError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )

@app.exception_handler(ValidationError)
async def validation_exception_handler(request, exc: ValidationError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )
    
@app.exception_handler(UnauthorizedError)
async def unauthorized_exception_handler(request, exc: UnauthorizedError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
        headers={"WWW-Authenticate": "Bearer"},
    )

@app.exception_handler(ForbiddenError)
async def forbidden_exception_handler(request, exc: ForbiddenError):
    return JSONResponse(
        status_code=exc.status_code,
        content=exc.detail,
    )

@app.exception_handler(PydanticValidationError)
async def pydantic_validation_exception_handler(request, exc: PydanticValidationError):
    errors = exc.errors()
    error_details = []
    for error in errors:
        loc = ".".join(map(str, error["loc"]))
        error_details.append(f"Field '{loc}': {error["msg"]}")
    return JSONResponse(
        status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
        content={"detail": "Validation error: " + "; ".join(error_details), "code": "PYDANTIC_VALIDATION_ERROR"},
    )

# --- Customer Endpoints (unchanged) ---
@app.post("/customers/", response_model=models.CustomerInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("supply_chain.write.customers"))]) # Permission changed
async def create_new_customer(
    customer: models.CustomerCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_customer(db_session, user_id, customer)

@app.get("/customers/{customer_id}", response_model=models.CustomerInDB,
             dependencies=[Depends(check_permission("supply_chain.read.customers"))]) # Permission changed
async def read_customer_by_id(
    customer_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_customer = await crud.get_customer(db_session, user_id, customer_id)
    if db_customer is None:
        raise NotFoundError(detail="Customer not found.")
    return db_customer

@app.get("/customers/"), response_model=List[models.CustomerInDB],
             dependencies=[Depends(check_permission("supply_chain.read.customers"))]) # Permission changed
async def read_all_customers(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_customers(db_session, user_id)

@app.put("/customers/{customer_id}", response_model=models.CustomerInDB,
             dependencies=[Depends(check_permission("supply_chain.write.customers"))]) # Permission changed
async def update_existing_customer(
    customer_id: str,
    customer: models.CustomerUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_customer = await crud.update_customer(db_session, user_id, customer_id, customer)
    if db_customer is None:
        raise NotFoundError(detail="Customer not found.")
    return db_customer

@app.delete("/customers/{customer_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("supply_chain.delete.customers"))]) # Permission changed
async def delete_existing_customer(
    customer_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_customer(db_session, user_id, customer_id)
    if not success:
        raise NotFoundError(detail="Customer not found.")
    return {"ok": True}

# --- Sales Invoice Endpoints (renamed from /invoices/) ---
@app.post("/sales-invoices/", response_model=models.SalesInvoiceInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("supply_chain.write.sales_invoices"))]) # Permission changed
async def create_new_sales_invoice(
    invoice: models.SalesInvoiceCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_sales_invoice(db_session, user_id, invoice)

@app.get("/sales-invoices/{invoice_id}", response_model=models.SalesInvoiceInDB,
             dependencies=[Depends(check_permission("supply_chain.read.sales_invoices"))]) # Permission changed
async def read_sales_invoice_by_id(
    invoice_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_invoice = await crud.get_sales_invoice(db_session, user_id, invoice_id)
    if db_invoice is None:
        raise NotFoundError(detail="Sales invoice not found.")
    return db_invoice

@app.get("/sales-invoices/"), response_model=List[models.SalesInvoiceInDB],
             dependencies=[Depends(check_permission("supply_chain.read.sales_invoices"))]) # Permission changed
async def read_all_sales_invoices(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_sales_invoices(db_session, user_id)

@app.put("/sales-invoices/{invoice_id}", response_model=models.SalesInvoiceInDB,
             dependencies=[Depends(check_permission("supply_chain.write.sales_invoices"))]) # Permission changed
async def update_existing_sales_invoice(
    invoice_id: str,
    invoice: models.SalesInvoiceUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_invoice = await crud.update_sales_invoice(db_session, user_id, invoice_id, invoice)
    if db_invoice is None:
        raise NotFoundError(detail="Sales invoice not found.")
    return db_invoice

@app.delete("/sales-invoices/{invoice_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("supply_chain.delete.sales_invoices"))]) # Permission changed
async def delete_existing_sales_invoice(
    invoice_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_sales_invoice(db_session, user_id, invoice_id)
    if not success:
        raise NotFoundError(detail="Sales invoice not found.")
    return {"ok": True}

# --- Supplier Endpoints (NEW) ---
@app.post("/suppliers/", response_model=models.SupplierInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("supply_chain.write.suppliers"))])
async def create_new_supplier(
    supplier: models.SupplierCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_supplier(db_session, user_id, supplier)

@app.get("/suppliers/{supplier_id}", response_model=models.SupplierInDB,
             dependencies=[Depends(check_permission("supply_chain.read.suppliers"))])
async def read_supplier_by_id(
    supplier_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_supplier = await crud.get_supplier(db_session, user_id, supplier_id)
    if db_supplier is None:
        raise NotFoundError(detail="Supplier not found.")
    return db_supplier

@app.get("/suppliers/"), response_model=List[models.SupplierInDB],
             dependencies=[Depends(check_permission("supply_chain.read.suppliers"))])
async def read_all_suppliers(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_suppliers(db_session, user_id)

@app.put("/suppliers/{supplier_id}", response_model=models.SupplierInDB,
             dependencies=[Depends(check_permission("supply_chain.write.suppliers"))])
async def update_existing_supplier(
    supplier_id: str,
    supplier: models.SupplierUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_supplier = await crud.update_supplier(db_session, user_id, supplier_id, supplier)
    if db_supplier is None:
        raise NotFoundError(detail="Supplier not found.")
    return db_supplier

@app.delete("/suppliers/{supplier_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("supply_chain.delete.suppliers"))])
async def delete_existing_supplier(
    supplier_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_supplier(db_session, user_id, supplier_id)
    if not success:
        raise NotFoundError(detail="Supplier not found.")
    return {"ok": True}

# --- Inventory Item Endpoints (NEW) ---
@app.post("/inventory-items/", response_model=models.InventoryItemInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("supply_chain.write.inventory_items"))])
async def create_new_inventory_item(
    item: models.InventoryItemCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_inventory_item(db_session, user_id, item)

@app.get("/inventory-items/{item_id}", response_model=models.InventoryItemInDB,
             dependencies=[Depends(check_permission("supply_chain.read.inventory_items"))])
async def read_inventory_item_by_id(
    item_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_item = await crud.get_inventory_item(db_session, user_id, item_id)
    if db_item is None:
        raise NotFoundError(detail="Inventory item not found.")
    return db_item

@app.get("/inventory-items/"), response_model=List[models.InventoryItemInDB],
             dependencies=[Depends(check_permission("supply_chain.read.inventory_items"))])
async def read_all_inventory_items(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_inventory_items(db_session, user_id)

@app.put("/inventory-items/{item_id}", response_model=models.InventoryItemInDB,
             dependencies=[Depends(check_permission("supply_chain.write.inventory_items"))])
async def update_existing_inventory_item(
    item_id: str,
    item: models.InventoryItemUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_item = await crud.update_inventory_item(db_session, user_id, item_id, item)
    if db_item is None:
        raise NotFoundError(detail="Inventory item not found.")
    return db_item

@app.delete("/inventory-items/{item_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("supply_chain.delete.inventory_items"))])
async def delete_existing_inventory_item(
    item_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_inventory_item(db_session, user_id, item_id)
    if not success:
        raise NotFoundError(detail="Inventory item not found.")
    return {"ok": True}

# --- Purchase Order Endpoints (NEW) ---
@app.post("/purchase-orders/", response_model=models.PurchaseOrderInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("supply_chain.write.purchase_orders"))])
async def create_new_purchase_order(
    po: models.PurchaseOrderCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_purchase_order(db_session, user_id, po)

@app.get("/purchase-orders/{po_id}", response_model=models.PurchaseOrderInDB,
             dependencies=[Depends(check_permission("supply_chain.read.purchase_orders"))])
async def read_purchase_order_by_id(
    po_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_po = await crud.get_purchase_order(db_session, user_id, po_id)
    if db_po is None:
        raise NotFoundError(detail="Purchase order not found.")
    return db_po

@app.get("/purchase-orders/"), response_model=List[models.PurchaseOrderInDB],
             dependencies=[Depends(check_permission("supply_chain.read.purchase_orders"))])
async def read_all_purchase_orders(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_purchase_orders(db_session, user_id)

@app.put("/purchase-orders/{po_id}", response_model=models.PurchaseOrderInDB,
             dependencies=[Depends(check_permission("supply_chain.write.purchase_orders"))])
async def update_existing_purchase_order(
    po_id: str,
    po: models.PurchaseOrderUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_po = await crud.update_purchase_order(db_session, user_id, po_id, po)
    if db_po is None:
        raise NotFoundError(detail="Purchase order not found.")
    return db_po

@app.delete("/purchase-orders/{po_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("supply_chain.delete.purchase_orders"))])
async def delete_existing_purchase_order(
    po_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_purchase_order(db_session, user_id, po_id)
    if not success:
        raise NotFoundError(detail="Purchase order not found.")
    return {"ok": True}

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "FinAcc Supply Chain Service is running!"}
