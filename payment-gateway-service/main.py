"""
FinAcc Payment Gateway Service
Processes payments, manages payment methods, handles refunds and disputes
"""

from fastapi import FastAPI, HTTPException, BackgroundTasks, Depends, status
from fastapi.responses import JSONResponse
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any
from datetime import datetime, timezone
from enum import Enum
import asyncio
import json
import uuid
import hashlib
from decimal import Decimal, ROUND_HALF_UP
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="FinAcc Payment Gateway Service",
    description="Payment processing, payment method management, refunds, and dispute handling",
    version="1.0.0",
)

# ============================================================================
# Configuration
# ============================================================================

SUPPORTED_CURRENCIES = ["USD", "EUR", "GBP", "CAD", "AUD", "JPY", "CNY"]
SUPPORTED_PAYMENT_METHODS = ["card", "bank_transfer", "ach", "wire", "check", "crypto"]
GATEWAY_PROVIDERS = {
    "stripe": {"name": "Stripe", "supports_cards": True, "supports_bank_transfer": True},
    "paypal": {"name": "PayPal", "supports_cards": True, "supports_bank_transfer": True},
    "square": {"name": "Square", "supports_cards": True, "supports_bank_transfer": False},
    "adyen": {"name": "Adyen", "supports_cards": True, "supports_bank_transfer": True},
    "braintree": {"name": "Braintree", "supports_cards": True, "supports_bank_transfer": True}
}

# ============================================================================
# Models
# ============================================================================

class PaymentStatus(str, Enum):
    PENDING = "pending"
    PROCESSING = "processing"
    AUTHORIZED = "authorized"
    CAPTURED = "captured"
    COMPLETED = "completed"
    FAILED = "failed"
    REFUNDED = "refunded"
    PARTIALLY_REFUNDED = "partially_refunded"
    CANCELLED = "cancelled"
    DISPUTED = "disputed"

class PaymentMethodType(str, Enum):
    CARD = "card"
    BANK_ACCOUNT = "bank_account"
    PAYPAL = "paypal"
    CRYPTO = "crypto"

class PaymentMethod(BaseModel):
    id: str
    customer_id: str
    type: PaymentMethodType
    is_default: bool = False
    # Card details (masked)
    last4: Optional[str] = None
    brand: Optional[str] = None  # visa, mastercard, amex
    expiry_month: Optional[int] = None
    expiry_year: Optional[int] = None
    # Bank details (masked)
    bank_name: Optional[str] = None
    account_type: Optional[str] = None  # checking, savings
    routing_number_last4: Optional[str] = None
    # Metadata
    billing_address: Optional[Dict[str, str]] = None
    metadata: Optional[Dict[str, Any]] = None
    created_at: datetime

class PaymentCreate(BaseModel):
    customer_id: str
    amount: float = Field(..., gt=0)
    currency: str = "USD"
    payment_method_id: Optional[str] = None
    payment_method_type: Optional[PaymentMethodType] = None
    # Card details if creating inline
    card_number: Optional[str] = None
    card_exp_month: Optional[int] = None
    card_exp_year: Optional[int] = None
    card_cvc: Optional[str] = None
    card_zip: Optional[str] = None
    # Options
    capture: bool = True
    description: Optional[str] = None
    order_id: Optional[str] = None
    metadata: Optional[Dict[str, Any]] = None

class Payment(BaseModel):
    id: str
    customer_id: str
    amount: float
    currency: str
    status: PaymentStatus
    payment_method_id: Optional[str] = None
    payment_method_type: Optional[PaymentMethodType] = None
    gateway_transaction_id: Optional[str] = None
    gateway_provider: Optional[str] = None
    description: Optional[str] = None
    order_id: Optional[str] = None
    refunded_amount: float = 0.0
    metadata: Optional[Dict[str, Any]] = None
    error_code: Optional[str] = None
    error_message: Optional[str] = None
    created_at: datetime
    updated_at: datetime
    completed_at: Optional[datetime] = None

class RefundCreate(BaseModel):
    payment_id: str
    amount: Optional[float] = None  # None = full refund
    reason: Optional[str] = None
    metadata: Optional[Dict[str, Any]] = None

class Refund(BaseModel):
    id: str
    payment_id: str
    amount: float
    currency: str
    status: str  # pending, completed, failed
    reason: Optional[str] = None
    gateway_refund_id: Optional[str] = None
    metadata: Optional[Dict[str, Any]] = None
    created_at: datetime
    completed_at: Optional[datetime] = None

class DisputeCreate(BaseModel):
    payment_id: str
    reason: str
    amount: Optional[float] = None
    evidence: Optional[Dict[str, Any]] = None
    metadata: Optional[Dict[str, Any]] = None

class Dispute(BaseModel):
    id: str
    payment_id: str
    amount: float
    currency: str
    reason: str
    status: str  # open, under_review, won, lost
    gateway_dispute_id: Optional[str] = None
    due_by: Optional[datetime] = None
    evidence: Optional[Dict[str, Any]] = None
    metadata: Optional[Dict[str, Any]] = None
    created_at: datetime
    updated_at: datetime

class Customer(BaseModel):
    id: str
    email: str
    name: Optional[str] = None
    default_currency: str = "USD"
    default_payment_method_id: Optional[str] = None
    metadata: Optional[Dict[str, Any]] = None
    created_at: datetime

# ============================================================================
# In-Memory Storage
# ============================================================================

customers: Dict[str, Customer] = {}
payment_methods: Dict[str, PaymentMethod] = {}
payments: Dict[str, Payment] = {}
refunds: Dict[str, Refund] = {}
disputes: Dict[str, Dispute] = {}

# ============================================================================
# Helper Functions
# ============================================================================

def round_amount(amount: float, currency: str = "USD") -> float:
    """Round amount to proper decimal places for currency"""
    decimal_places = {
        "USD": 2, "EUR": 2, "GBP": 2, "CAD": 2, "AUD": 2,
        "JPY": 0, "CNY": 2
    }
    places = decimal_places.get(currency, 2)
    return float(Decimal(str(amount)).quantize(Decimal(f"0.{'0' * places}"), rounding=ROUND_HALF_UP))

def generate_payment_id() -> str:
    """Generate unique payment ID"""
    return f"pay_{uuid.uuid4().hex[:24]}"

def generate_customer_id() -> str:
    """Generate unique customer ID"""
    return f"cus_{uuid.uuid4().hex[:16]}"

def mask_card_number(card_number: str) -> str:
    """Mask card number, showing only last 4 digits"""
    if len(card_number) < 4:
        return "****"
    return card_number[-4:]

def detect_card_brand(card_number: str) -> str:
    """Detect card brand from card number"""
    card_number = card_number.replace(" ", "").replace("-", "")

    if card_number.startswith("4"):
        return "visa"
    elif card_number.startswith(("51", "52", "53", "54", "55")):
        return "mastercard"
    elif card_number.startswith(("34", "37")):
        return "amex"
    elif card_number.startswith("6011") or card_number.startswith(("644", "645", "646", "647", "648", "649")) or card_number.startswith("65"):
        return "discover"
    else:
        return "unknown"

def validate_card_expiry(month: int, year: int) -> bool:
    """Validate card expiry date"""
    now = datetime.now(timezone.utc)
    current_year = now.year
    current_month = now.month

    if year < current_year:
        return False
    if year == current_year and month < current_month:
        return False

    return True

async def process_payment_gateway(
    payment: Payment,
    method: PaymentMethod,
    capture: bool
) -> Dict[str, Any]:
    """Process payment through gateway (simulated)"""
    # In production, integrate with actual gateway (Stripe, PayPal, etc.)
    # This is a simulation for the service

    # Simulate processing delay
    await asyncio.sleep(0.5)

    # Simulate gateway response
    if payment.amount > 10000:
        return {
            "success": False,
            "error_code": "insufficient_funds",
            "error_message": "Payment amount exceeds limit"
        }

    return {
        "success": True,
        "gateway_transaction_id": f"ch_{uuid.uuid4().hex[:24]}",
        "status": "captured" if capture else "authorized"
    }

async def process_refund_gateway(refund: Refund) -> Dict[str, Any]:
    """Process refund through gateway (simulated)"""
    await asyncio.sleep(0.3)

    return {
        "success": True,
        "gateway_refund_id": f"re_{uuid.uuid4().hex[:24]}",
        "status": "completed"
    }

# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "payment-gateway",
        "total_payments": len(payments),
        "total_customers": len(customers)
    }

# --- Customer Management ---

@app.post("/customers", status_code=status.HTTP_201_CREATED)
async def create_customer(
    email: str,
    name: Optional[str] = None,
    currency: str = "USD",
    metadata: Optional[Dict[str, Any]] = None
):
    """Create a new customer"""
    if currency not in SUPPORTED_CURRENCIES:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Currency not supported. Supported: {', '.join(SUPPORTED_CURRENCIES)}"
        )

    customer_id = generate_customer_id()
    now = datetime.now(timezone.utc)

    customer = Customer(
        id=customer_id,
        email=email,
        name=name,
        default_currency=currency,
        metadata=metadata,
        created_at=now
    )

    customers[customer_id] = customer
    return customer

@app.get("/customers/{customer_id}")
async def get_customer(customer_id: str):
    """Get customer details"""
    if customer_id not in customers:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Customer not found")
    return customers[customer_id]

@app.get("/customers")
async def list_customers(limit: int = 50, offset: int = 0):
    """List all customers"""
    results = list(customers.values())
    results.sort(key=lambda x: x.created_at, reverse=True)
    total = len(results)
    results = results[offset:offset + limit]
    return {"total": total, "customers": results}

# --- Payment Method Management ---

@app.post("/customers/{customer_id}/payment-methods", status_code=status.HTTP_201_CREATED)
async def add_payment_method(
    customer_id: str,
    method_type: PaymentMethodType,
    # Card details
    card_number: Optional[str] = None,
    card_exp_month: Optional[int] = None,
    card_exp_year: Optional[int] = None,
    card_zip: Optional[str] = None,
    # Bank details
    bank_name: Optional[str] = None,
    account_number: Optional[str] = None,
    routing_number: Optional[str] = None,
    account_type: Optional[str] = None,
    # Options
    set_as_default: bool = False,
    metadata: Optional[Dict[str, Any]] = None
):
    """Add a payment method for customer"""
    if customer_id not in customers:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Customer not found")

    method_id = f"pm_{uuid.uuid4().hex[:24]}"
    now = datetime.now(timezone.utc)

    method_data = {
        "id": method_id,
        "customer_id": customer_id,
        "type": method_type,
        "is_default": set_as_default,
        "metadata": metadata,
        "created_at": now
    }

    if method_type == PaymentMethodType.CARD:
        if not card_number:
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Card number required")
        if not validate_card_expiry(card_exp_month, card_exp_year):
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Card expired")

        method_data["last4"] = mask_card_number(card_number)
        method_data["brand"] = detect_card_brand(card_number)
        method_data["expiry_month"] = card_exp_month
        method_data["expiry_year"] = card_exp_year
        method_data["billing_address"] = {"zip": card_zip} if card_zip else None

    elif method_type == PaymentMethodType.BANK_ACCOUNT:
        if not account_number or not routing_number:
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Bank details required")

        method_data["bank_name"] = bank_name
        method_data["last4"] = account_number[-4:] if len(account_number) >= 4 else account_number
        method_data["account_type"] = account_type
        method_data["routing_number_last4"] = routing_number[-4:] if len(routing_number) >= 4 else routing_number

    method = PaymentMethod(**method_data)
    payment_methods[method_id] = method

    # Update customer's default payment method
    if set_as_default:
        if customers[customer_id].default_payment_method_id:
            old_default_id = customers[customer_id].default_payment_method_id
            if old_default_id in payment_methods:
                payment_methods[old_default_id].is_default = False

        customers[customer_id].default_payment_method_id = method_id

    return method

@app.get("/customers/{customer_id}/payment-methods")
async def list_payment_methods(customer_id: str):
    """List customer's payment methods"""
    if customer_id not in customers:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Customer not found")

    methods = [m for m in payment_methods.values() if m.customer_id == customer_id]
    return {"total": len(methods), "payment_methods": methods}

@app.delete("/payment-methods/{method_id}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_payment_method(method_id: str):
    """Delete a payment method"""
    if method_id not in payment_methods:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Payment method not found")

    del payment_methods[method_id]
    return {"ok": True}

# --- Payment Processing ---

@app.post("/payments", status_code=status.HTTP_201_CREATED)
async def create_payment(
    payment_data: PaymentCreate,
    background_tasks: BackgroundTasks
):
    """Create and process a payment"""
    if payment_data.currency not in SUPPORTED_CURRENCIES:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Currency not supported"
        )

    if payment_data.customer_id not in customers:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Customer not found")

    # Get or validate payment method
    method = None
    if payment_data.payment_method_id:
        if payment_data.payment_method_id not in payment_methods:
            raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Payment method not found")
        method = payment_methods[payment_data.payment_method_id]
    elif payment_data.payment_method_type:
        # Create inline payment method
        method_type = payment_data.payment_method_type

        if method_type == PaymentMethodType.CARD:
            if not all([payment_data.card_number, payment_data.card_exp_month, payment_data.card_exp_year]):
                raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Card details required")

            if not validate_card_expiry(payment_data.card_exp_month, payment_data.card_exp_year):
                raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Card expired")

            method_id = f"pm_{uuid.uuid4().hex[:24]}"
            method = PaymentMethod(
                id=method_id,
                customer_id=payment_data.customer_id,
                type=method_type,
                last4=mask_card_number(payment_data.card_number),
                brand=detect_card_brand(payment_data.card_number),
                expiry_month=payment_data.card_exp_month,
                expiry_year=payment_data.card_exp_year,
                created_at=datetime.now(timezone.utc)
            )
            payment_methods[method_id] = method
        else:
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Payment method type not fully supported")
    else:
        # Use customer's default payment method
        default_pm_id = customers[payment_data.customer_id].default_payment_method_id
        if not default_pm_id or default_pm_id not in payment_methods:
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="No payment method available")
        method = payment_methods[default_pm_id]

    # Create payment record
    payment_id = generate_payment_id()
    now = datetime.now(timezone.utc)

    payment = Payment(
        id=payment_id,
        customer_id=payment_data.customer_id,
        amount=round_amount(payment_data.amount, payment_data.currency),
        currency=payment_data.currency,
        status=PaymentStatus.PROCESSING,
        payment_method_id=method.id,
        payment_method_type=method.type,
        description=payment_data.description,
        order_id=payment_data.order_id,
        metadata=payment_data.metadata,
        created_at=now,
        updated_at=now
    )

    payments[payment_id] = payment

    # Process payment in background
    background_tasks.add_task(process_payment, payment_id, method.id, payment_data.capture)

    return payment

async def process_payment(payment_id: str, method_id: str, capture: bool):
    """Background task to process payment"""
    payment = payments[payment_id]
    method = payment_methods[method_id]

    try:
        # Call gateway
        result = await process_payment_gateway(payment, method, capture)

        if result["success"]:
            payment.gateway_transaction_id = result["gateway_transaction_id"]
            payment.gateway_provider = "stripe"  # Simulated
            payment.status = PaymentStatus.CAPTURED if capture else PaymentStatus.AUTHORIZED
            payment.completed_at = datetime.now(timezone.utc)
        else:
            payment.status = PaymentStatus.FAILED
            payment.error_code = result["error_code"]
            payment.error_message = result["error_message"]

    except Exception as e:
        payment.status = PaymentStatus.FAILED
        payment.error_code = "processing_error"
        payment.error_message = str(e)

    payment.updated_at = datetime.now(timezone.utc)

@app.get("/payments/{payment_id}")
async def get_payment(payment_id: str):
    """Get payment details"""
    if payment_id not in payments:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Payment not found")
    return payments[payment_id]

@app.get("/payments")
async def list_payments(
    customer_id: Optional[str] = None,
    status: Optional[PaymentStatus] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 50,
    offset: int = 0
):
    """List payments with filters"""
    results = list(payments.values())

    if customer_id:
        results = [p for p in results if p.customer_id == customer_id]
    if status:
        results = [p for p in results if p.status == status]
    if start_date:
        results = [p for p in results if p.created_at >= start_date]
    if end_date:
        results = [p for p in results if p.created_at <= end_date]

    results.sort(key=lambda x: x.created_at, reverse=True)
    total = len(results)
    results = results[offset:offset + limit]

    return {"total": total, "payments": results}

@app.post("/payments/{payment_id}/capture", status_code=status.HTTP_200_OK)
async def capture_authorized_payment(payment_id: str):
    """Capture an authorized payment"""
    if payment_id not in payments:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Payment not found")

    payment = payments[payment_id]

    if payment.status != PaymentStatus.AUTHORIZED:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Cannot capture payment with status: {payment.status}"
        )

    # In production, call gateway to capture
    payment.status = PaymentStatus.CAPTURED
    payment.completed_at = datetime.now(timezone.utc)
    payment.updated_at = datetime.now(timezone.utc)

    return payment

@app.post("/payments/{payment_id}/cancel", status_code=status.HTTP_200_OK)
async def cancel_payment(payment_id: str):
    """Cancel a payment"""
    if payment_id not in payments:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Payment not found")

    payment = payments[payment_id]

    if payment.status in [PaymentStatus.COMPLETED, PaymentStatus.REFUNDED]:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Cannot cancel payment with status: {payment.status}"
        )

    # In production, void with gateway
    payment.status = PaymentStatus.CANCELLED
    payment.updated_at = datetime.now(timezone.utc)

    return payment

# --- Refunds ---

@app.post("/refunds", status_code=status.HTTP_201_CREATED)
async def create_refund(
    refund_data: RefundCreate,
    background_tasks: BackgroundTasks
):
    """Create and process a refund"""
    if refund_data.payment_id not in payments:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Payment not found")

    payment = payments[refund_data.payment_id]

    if payment.status not in [PaymentStatus.CAPTURED, PaymentStatus.COMPLETED]:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Cannot refund payment with status: {payment.status}"
        )

    # Calculate refund amount
    refund_amount = refund_data.amount or (payment.amount - payment.refunded_amount)

    if refund_amount > (payment.amount - payment.refunded_amount):
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Refund amount exceeds available"
        )

    refund_id = f"re_{uuid.uuid4().hex[:24]}"
    now = datetime.now(timezone.utc)

    refund = Refund(
        id=refund_id,
        payment_id=refund_data.payment_id,
        amount=round_amount(refund_amount, payment.currency),
        currency=payment.currency,
        status="pending",
        reason=refund_data.reason,
        metadata=refund_data.metadata,
        created_at=now
    )

    refunds[refund_id] = refund

    # Process refund in background
    background_tasks.add_task(process_refund, refund_id)

    return refund

async def process_refund(refund_id: str):
    """Background task to process refund"""
    refund = refunds[refund_id]
    payment = payments[refund.payment_id]

    try:
        # Call gateway
        result = await process_refund_gateway(refund)

        if result["success"]:
            refund.gateway_refund_id = result["gateway_refund_id"]
            refund.status = "completed"
            refund.completed_at = datetime.now(timezone.utc)

            # Update payment
            payment.refunded_amount += refund.amount
            if payment.refunded_amount >= payment.amount:
                payment.status = PaymentStatus.REFUNDED
            else:
                payment.status = PaymentStatus.PARTIALLY_REFUNDED
        else:
            refund.status = "failed"

    except Exception as e:
        refund.status = "failed"
        refund.metadata = refund.metadata or {}
        refund.metadata["error"] = str(e)

    payment.updated_at = datetime.now(timezone.utc)

@app.get("/refunds/{refund_id}")
async def get_refund(refund_id: str):
    """Get refund details"""
    if refund_id not in refunds:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Refund not found")
    return refunds[refund_id]

@app.get("/payments/{payment_id}/refunds")
async def list_payment_refunds(payment_id: str):
    """List all refunds for a payment"""
    if payment_id not in payments:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Payment not found")

    payment_refunds = [r for r in refunds.values() if r.payment_id == payment_id]
    return {"total": len(payment_refunds), "refunds": payment_refunds}

# --- Disputes ---

@app.post("/disputes", status_code=status.HTTP_201_CREATED)
async def create_dispute(dispute_data: DisputeCreate):
    """Create a dispute"""
    if dispute_data.payment_id not in payments:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Payment not found")

    payment = payments[dispute_data.payment_id]

    if payment.status != PaymentStatus.CAPTURED:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="Can only dispute captured payments"
        )

    dispute_id = f"dp_{uuid.uuid4().hex[:24]}"
    now = datetime.now(timezone.utc)

    dispute = Dispute(
        id=dispute_id,
        payment_id=dispute_data.payment_id,
        amount=dispute_data.amount or payment.amount,
        currency=payment.currency,
        reason=dispute_data.reason,
        status="open",
        evidence=dispute_data.evidence,
        metadata=dispute_data.metadata,
        due_by=now + timedelta(days=14),
        created_at=now,
        updated_at=now
    )

    disputes[dispute_id] = dispute

    # Update payment status
    payment.status = PaymentStatus.DISPUTED
    payment.updated_at = now

    return dispute

@app.get("/disputes/{dispute_id}")
async def get_dispute(dispute_id: str):
    """Get dispute details"""
    if dispute_id not in disputes:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Dispute not found")
    return disputes[dispute_id]

@app.get("/disputes")
async def list_disputes(
    payment_id: Optional[str] = None,
    status: Optional[str] = None,
    limit: int = 50
):
    """List disputes"""
    results = list(disputes.values())

    if payment_id:
        results = [d for d in results if d.payment_id == payment_id]
    if status:
        results = [d for d in results if d.status == status]

    results.sort(key=lambda x: x.created_at, reverse=True)
    return {"total": len(results), "disputes": results[:limit]}

@app.post("/disputes/{dispute_id}/evidence")
async def submit_dispute_evidence(dispute_id: str, evidence: Dict[str, Any]):
    """Submit evidence for dispute"""
    if dispute_id not in disputes:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Dispute not found")

    dispute = disputes[dispute_id]
    dispute.evidence = evidence
    dispute.updated_at = datetime.now(timezone.utc)

    # In production, submit to gateway
    return dispute

# --- Statistics ---

@app.get("/statistics")
async def get_statistics(start_date: Optional[datetime] = None, end_date: Optional[datetime] = None):
    """Get payment gateway statistics"""
    all_payments = list(payments.values())

    if start_date:
        all_payments = [p for p in all_payments if p.created_at >= start_date]
    if end_date:
        all_payments = [p for p in all_payments if p.created_at <= end_date]

    total_amount = sum(p.amount for p in all_payments)
    successful_amount = sum(p.amount for p in all_payments if p.status in [PaymentStatus.CAPTURED, PaymentStatus.COMPLETED])
    refunded_amount = sum(p.refunded_amount for p in all_payments)

    by_status = {}
    for payment in all_payments:
        status_key = payment.status.value
        if status_key not in by_status:
            by_status[status_key] = {"count": 0, "amount": 0.0}
        by_status[status_key]["count"] += 1
        by_status[status_key]["amount"] += payment.amount

    by_currency = {}
    for payment in all_payments:
        currency = payment.currency
        if currency not in by_currency:
            by_currency[currency] = {"count": 0, "amount": 0.0}
        by_currency[currency]["count"] += 1
        by_currency[currency]["amount"] += payment.amount

    return {
        "total_payments": len(all_payments),
        "total_amount": total_amount,
        "successful_amount": successful_amount,
        "refunded_amount": refunded_amount,
        "success_rate": round(successful_amount / total_amount * 100, 2) if total_amount else 0,
        "by_status": by_status,
        "by_currency": by_currency,
        "total_customers": len(customers),
        "total_payment_methods": len(payment_methods),
        "open_disputes": len([d for d in disputes.values() if d.status == "open"])
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8098)