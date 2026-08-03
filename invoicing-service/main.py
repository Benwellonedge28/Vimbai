from fastapi import FastAPI, Depends, HTTPException, status, Query
from fastapi.responses import JSONResponse # NEW
from typing import List, Optional
from neo4j import AsyncSession
from invoicing_service import models, crud
from invoicing_service.database import init_db_schema, Neo4jConnector
from invoicing_service.dependencies import get_db_session, get_user_id, get_jwt_token
from invoicing_service.utils.auth import check_permission
from invoicing_service.exceptions import NotFoundError, ConflictError, ValidationError, UnauthorizedError, ForbiddenError # NEW
import os
from dotenv import load_dotenv
from datetime import datetime
from decimal import Decimal
from pydantic import ValidationError as PydanticValidationError # NEW: to catch pydantic's internal validation errors

# Load environment variables
load_dotenv()

app = FastAPI(
    title="Vimbai Invoicing Service",
    description="Manages customers, invoices, and payments.",
    version="0.1.0",
)

@app.on_event("startup")
async def startup_event():
    Neo4jConnector.get_driver()
    await init_db_schema()

@app.on_event("shutdown")
async def shutdown_event():
    Neo4jConnector.close_driver()

# --- Global Exception Handlers (NEW) ---
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

@app.exception_handler(PydanticValidationError) # NEW: Catch Pydantic's internal validation errors
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

# --- Customer Endpoints ---
@app.post("/customers/", response_model=models.CustomerInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("invoicing.write.customers"))])
async def create_new_customer(
    customer: models.CustomerCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_customer = await crud.get_customer_by_id(db_session, customer.customer_id, user_id)
    if db_customer:
        raise ConflictError(detail="Customer with this ID already exists for user.", code="CUSTOMER_EXISTS")
    return await crud.create_customer(db_session, user_id, customer)

@app.get("/customers/", response_model=List[models.CustomerInDB],
             dependencies=[Depends(check_permission("invoicing.read.customers"))])
async def read_all_customers(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_customers(db_session, user_id)

@app.get("/customers/{customer_id}", response_model=models.CustomerInDB,
             dependencies=[Depends(check_permission("invoicing.read.customers"))])
async def read_customer_by_id(
    customer_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_customer = await crud.get_customer_by_id(db_session, customer_id, user_id)
    if db_customer is None:
        raise NotFoundError(detail="Customer not found.", code="CUSTOMER_NOT_FOUND")
    return db_customer
    
@app.put("/customers/{customer_id}", response_model=models.CustomerInDB,
             dependencies=[Depends(check_permission("invoicing.write.customers"))])
async def update_existing_customer(
    customer_id: str,
    customer: models.CustomerUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_customer = await crud.update_customer(db_session, customer_id, user_id, customer)
    if db_customer is None:
        raise NotFoundError(detail="Customer not found.", code="CUSTOMER_NOT_FOUND")
    return db_customer

@app.delete("/customers/{customer_id}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("invoicing.delete.customers"))])
async def delete_existing_customer(
    customer_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_customer(db_session, customer_id, user_id)
    if not success:
        raise NotFoundError(detail="Customer not found.", code="CUSTOMER_NOT_FOUND")
    return {"ok": True}

# --- Invoice Endpoints ---
@app.post("/invoices/", response_model=models.InvoiceInDB, status_code=status.HTTP_201_CREATED,
              dependencies=[Depends(check_permission("invoicing.write.invoices"))])
async def create_new_invoice(
    invoice: models.InvoiceCreate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_customer = await crud.get_customer_by_id(db_session, invoice.customer_id, user_id)
    if db_customer is None:
        raise ValidationError(detail="Customer not found.", code="CUSTOMER_NOT_FOUND_FOR_INVOICE")
    
    db_invoice = await crud.get_invoice_by_number(db_session, invoice.invoice_number, user_id)
    if db_invoice:
        raise ConflictError(detail="Invoice with this number already exists.", code="INVOICE_EXISTS")
    
    return await crud.create_invoice(db_session, user_id, invoice)

@app.get("/invoices/", response_model=List[models.InvoiceInDB],
             dependencies=[Depends(check_permission("invoicing.read.invoices"))])
async def read_all_invoices(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_all_invoices(db_session, user_id)

@app.get("/invoices/{invoice_number}", response_model=models.InvoiceInDB,
             dependencies=[Depends(check_permission("invoicing.read.invoices"))])
async def read_invoice_by_number(
    invoice_number: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_invoice = await crud.get_invoice_by_number(db_session, invoice_number, user_id)
    if db_invoice is None:
        raise NotFoundError(detail="Invoice not found.", code="INVOICE_NOT_FOUND")
    return db_invoice
    
@app.put("/invoices/{invoice_number}", response_model=models.InvoiceInDB,
             dependencies=[Depends(check_permission("invoicing.write.invoices"))])
async def update_existing_invoice(
    invoice_number: str,
    invoice: models.InvoiceUpdate,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    db_invoice = await crud.update_invoice(db_session, invoice_number, user_id, invoice)
    if db_invoice is None:
        raise NotFoundError(detail="Invoice not found.", code="INVOICE_NOT_FOUND")
    return db_invoice

@app.delete("/invoices/{invoice_number}", status_code=status.HTTP_204_NO_CONTENT,
                dependencies=[Depends(check_permission("invoicing.delete.invoices"))])
async def delete_existing_invoice(
    invoice_number: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session)
):
    success = await crud.delete_invoice(db_session, invoice_number, user_id)
    if not success:
        raise NotFoundError(detail="Invoice not found.", code="INVOICE_NOT_FOUND")
    return {"ok": True}

@app.post("/invoices/{invoice_number}/record-payment", response_model=models.CreateJournalEntryResponse,
              dependencies=[Depends(check_permission("invoicing.record.payment"))])
async def record_invoice_payment(
    invoice_number: str,
    payment_amount: Decimal = Query(..., description="Amount of the payment"), # Modified to be a Query parameter
    payment_date: datetime = Query(default_factory=datetime.utcnow, description="Date of the payment (ISO format)"), # Modified to be a Query parameter
    user_id: str = Depends(get_user_id),
    jwt_token: str = Depends(get_jwt_token),
    db_session: AsyncSession = Depends(get_db_session)
):
    try:
        return await crud.record_payment_for_invoice(db_session, invoice_number, user_id, payment_amount, payment_date, jwt_token)
    except ValidationError as e: # MODIFIED to catch ValidationError
        raise e # Re-raise if it's already a ValidationError
    except Exception as e: # Catch other potential errors
        raise ValidationError(detail=str(e), code="INVOICE_PAYMENT_ERROR") # Wrap as ValidationError

# --- Root endpoint for health check ---
@app.get("/")
async def read_root():
    return {"message": "Vimbai Invoicing Service is running!"}
