"""
Vimbai Petty Cash Book Service
Dedicated service for petty cash management with full audit trail
Supports multiple petty cash funds, reimbursement workflows, and integration
with main accounting system
"""

from fastapi import FastAPI, HTTPException, Depends, BackgroundTasks, Request
from pydantic import BaseModel, Field
from typing import List, Optional, Dict, Any
from datetime import datetime, timezone, timedelta
from enum import Enum
import uuid
from decimal import Decimal

app = FastAPI(
    title="Vimbai Petty Cash Book Service",
    description="Comprehensive petty cash management with fund tracking, reimbursement workflows, and accounting integration",
    version="1.0.0",
)

# ============================================================================
# Enums
# ============================================================================

class PettyCashStatus(str, Enum):
    ACTIVE = "active"
    REPLENISHING = "replenishing"
    CLOSED = "closed"
    REIMBURSING = "reimbursing"


class TransactionType(str, Enum):
    RECEIPT = "receipt"
    PAYMENT = "payment"
    REPLENISHMENT = "replenishment"
    INITIAL_FUND = "initial_fund"
    ADJUSTMENT = "adjustment"
    CLOSING = "closing"


class PaymentCategory(str, Enum):
    TRAVEL = "travel"
    OFFICE_SUPPLIES = "office_supplies"
    POSTAGE = "postage"
    PRINTING = "printing"
    MEALS = "meals"
    TRANSPORTATION = "transportation"
    MISCELLANEOUS = "miscellaneous"
    STATIONERY = "stationery"
    TELEPHONE = "telephone"
    OFFICE_EXPENSES = "office_expenses"


class ReimbursementStatus(str, Enum):
    PENDING = "pending"
    APPROVED = "approved"
    REJECTED = "rejected"
    PROCESSED = "processed"
    CANCELLED = "cancelled"


# ============================================================================
# Pydantic Models
# ============================================================================

class PettyCashFund(BaseModel):
    id: str
    fund_code: str
    fund_name: str
    custodian_id: str
    custodian_name: str
    location: str
    maximum_balance: Decimal
    minimum_balance: Decimal
    replenishment_threshold: Decimal
    replenishment_amount: Decimal
    status: PettyCashStatus = PettyCashStatus.ACTIVE
    account_code: str  # Link to main accounting
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PettyCashTransaction(BaseModel):
    id: str
    fund_id: str
    transaction_type: TransactionType
    amount: Decimal
    date: datetime
    description: str
    category: PaymentCategory
    recipient_name: Optional[str] = None
    recipient_id: Optional[str] = None
    reference_number: str
    voucher_number: str
    approved_by: Optional[str] = None
    entered_by: str
    receipt_attachment: Optional[str] = None
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PettyCashReplenishment(BaseModel):
    id: str
    fund_id: str
    amount: Decimal
    request_date: datetime
    requested_by: str
    approved_by: Optional[str] = None
    approved_date: Optional[datetime] = None
    status: ReimbursementStatus = ReimbursementStatus.PENDING
    transactions_included: List[str] = []  # Transaction IDs
    total_cash_disbursed: Decimal = Decimal("0")
    bank_reference: Optional[str] = None
    notes: Optional[str] = None


class PettyCashSummary(BaseModel):
    fund_id: str
    fund_name: str
    opening_balance: Decimal
    total_receipts: Decimal
    total_payments: Decimal
    closing_balance: Decimal
    outstanding_vouchers: int
    available_cash: Decimal
    replenishment_needed: bool
    last_replenishment_date: Optional[datetime] = None


class PettyCashVoucher(BaseModel):
    id: str
    fund_id: str
    voucher_number: str
    date: datetime
    payee: str
    amount: Decimal
    description: str
    category: PaymentCategory
    approved_by: Optional[str] = None
    receipt_attached: bool = False
    status: str = "pending"
    entered_by: str
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


# ============================================================================
# Storage
# ============================================================================

petty_cash_funds: Dict[str, PettyCashFund] = {}
petty_cash_transactions: Dict[str, PettyCashTransaction] = {}
petty_cash_replenishments: Dict[str, PettyCashReplenishment] = {}
petty_cash_vouchers: Dict[str, PettyCashVoucher] = {}


# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    """Health check endpoint"""
    total_funds = len(petty_cash_funds)
    active_funds = sum(1 for f in petty_cash_funds.values() if f.status == PettyCashStatus.ACTIVE)
    total_transactions = len(petty_cash_transactions)

    return {
        "status": "healthy",
        "service": "petty-cash",
        "version": "1.0.0",
        "total_funds": total_funds,
        "active_funds": active_funds,
        "total_transactions": total_transactions,
    }


# --- Fund Management ---

@app.post("/funds")
async def create_petty_cash_fund(fund: PettyCashFund):
    """Create a new petty cash fund"""
    fund.id = str(uuid.uuid4())
    fund.created_at = datetime.now(timezone.utc)
    fund.updated_at = datetime.now(timezone.utc)

    petty_cash_funds[fund.id] = fund
    return fund


@app.get("/funds")
async def list_petty_cash_funds(
    status: Optional[PettyCashStatus] = None,
    custodian_id: Optional[str] = None
):
    """List all petty cash funds"""
    results = list(petty_cash_funds.values())

    if status:
        results = [f for f in results if f.status == status]
    if custodian_id:
        results = [f for f in results if f.custodian_id == custodian_id]

    return results


@app.get("/funds/{fund_id}")
async def get_petty_cash_fund(fund_id: str):
    """Get petty cash fund details"""
    if fund_id not in petty_cash_funds:
        raise HTTPException(status_code=404, detail="Fund not found")
    return petty_cash_funds[fund_id]


@app.put("/funds/{fund_id}")
async def update_petty_cash_fund(fund_id: str, fund: PettyCashFund):
    """Update petty cash fund"""
    if fund_id not in petty_cash_funds:
        raise HTTPException(status_code=404, detail="Fund not found")

    fund.id = fund_id
    fund.updated_at = datetime.now(timezone.utc)
    petty_cash_funds[fund_id] = fund
    return fund


@app.post("/funds/{fund_id}/close")
async def close_petty_cash_fund(fund_id: str, closed_by: str):
    """Close a petty cash fund"""
    if fund_id not in petty_cash_funds:
        raise HTTPException(status_code=404, detail="Fund not found")

    fund = petty_cash_funds[fund_id]
    fund.status = PettyCashStatus.CLOSED
    fund.updated_at = datetime.now(timezone.utc)

    return {"status": "closed", "fund_id": fund_id, "closed_by": closed_by}


# --- Transaction Management ---

@app.post("/transactions")
async def create_petty_cash_transaction(
    transaction: PettyCashTransaction,
    request: Request = None
):
    """Create petty cash transaction"""
    transaction.id = str(uuid.uuid4())
    transaction.created_at = datetime.now(timezone.utc)

    # Validate fund exists
    if transaction.fund_id not in petty_cash_funds:
        raise HTTPException(status_code=404, detail="Fund not found")

    # Check fund balance for payments
    fund = petty_cash_funds[transaction.fund_id]
    current_balance = await get_fund_balance(transaction.fund_id)

    if transaction.transaction_type == TransactionType.PAYMENT:
        if current_balance - transaction.amount < fund.minimum_balance:
            raise HTTPException(
                status_code=400,
                detail=f"Insufficient balance. Minimum balance is {fund.minimum_balance}"
            )

    petty_cash_transactions[transaction.id] = transaction
    return transaction


@app.get("/transactions")
async def list_petty_cash_transactions(
    fund_id: Optional[str] = None,
    transaction_type: Optional[TransactionType] = None,
    category: Optional[PaymentCategory] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100
):
    """List petty cash transactions"""
    results = list(petty_cash_transactions.values())

    if fund_id:
        results = [t for t in results if t.fund_id == fund_id]
    if transaction_type:
        results = [t for t in results if t.transaction_type == transaction_type]
    if category:
        results = [t for t in results if t.category == category]
    if start_date:
        results = [t for t in results if t.date >= start_date]
    if end_date:
        results = [t for t in results if t.date <= end_date]

    results.sort(key=lambda x: x.date, reverse=True)
    return results[:limit]


@app.get("/transactions/{transaction_id}")
async def get_petty_cash_transaction(transaction_id: str):
    """Get transaction details"""
    if transaction_id not in petty_cash_transactions:
        raise HTTPException(status_code=404, detail="Transaction not found")
    return petty_cash_transactions[transaction_id]


@app.get("/funds/{fund_id}/balance")
async def get_fund_balance(fund_id: str):
    """Get current fund balance"""
    if fund_id not in petty_cash_funds:
        raise HTTPException(status_code=404, detail="Fund not found")

    transactions = [t for t in petty_cash_transactions.values() if t.fund_id == fund_id]

    total_receipts = sum(
        t.amount for t in transactions
        if t.transaction_type in [TransactionType.RECEIPT, TransactionType.REPLENISHMENT, TransactionType.INITIAL_FUND]
    )
    total_payments = sum(
        t.amount for t in transactions
        if t.transaction_type == TransactionType.PAYMENT
    )

    return {
        "fund_id": fund_id,
        "total_receipts": str(total_receipts),
        "total_payments": str(total_payments),
        "current_balance": str(total_receipts - total_payments),
    }


@app.get("/funds/{fund_id}/summary")
async def get_fund_summary(fund_id: str):
    """Get fund summary"""
    if fund_id not in petty_cash_funds:
        raise HTTPException(status_code=404, detail="Fund not found")

    fund = petty_cash_funds[fund_id]
    transactions = [t for t in petty_cash_transactions.values() if t.fund_id == fund_id]
    balance_info = await get_fund_balance(fund_id)

    receipts = [t for t in transactions if t.transaction_type in [TransactionType.RECEIPT, TransactionType.REPLENISHMENT, TransactionType.INITIAL_FUND]]
    payments = [t for t in transactions if t.transaction_type == TransactionType.PAYMENT]

    pending_vouchers = sum(1 for t in transactions if t.status == "pending" if hasattr(t, "status"))

    # Check if replenishment needed
    current_balance = Decimal(balance_info["current_balance"])
    replenishment_needed = current_balance < fund.replenishment_threshold

    # Get last replenishment
    replenishments = [t for t in transactions if t.transaction_type == TransactionType.REPLENISHMENT]
    last_repl = max((t.date for t in replenishments), default=None)

    summary = PettyCashSummary(
        fund_id=fund_id,
        fund_name=fund.fund_name,
        opening_balance=Decimal("0"),
        total_receipts=Decimal(balance_info["total_receipts"]),
        total_payments=Decimal(balance_info["total_payments"]),
        closing_balance=current_balance,
        outstanding_vouchers=len(pending_vouchers) if pending_vouchers else 0,
        available_cash=current_balance,
        replenishment_needed=replenishment_needed,
        last_replenishment_date=last_repl,
    )

    return summary


# --- Voucher Management ---

@app.post("/vouchers")
async def create_petty_cash_voucher(voucher: PettyCashVoucher):
    """Create petty cash voucher"""
    voucher.id = str(uuid.uuid4())
    voucher.created_at = datetime.now(timezone.utc)

    petty_cash_vouchers[voucher.id] = voucher

    # Create corresponding transaction
    transaction = PettyCashTransaction(
        id=str(uuid.uuid4()),
        fund_id=voucher.fund_id,
        transaction_type=TransactionType.PAYMENT,
        amount=voucher.amount,
        date=voucher.date,
        description=voucher.description,
        category=voucher.category,
        recipient_name=voucher.payee,
        voucher_number=voucher.voucher_number,
        entered_by=voucher.entered_by,
        created_at=datetime.now(timezone.utc),
    )
    petty_cash_transactions[transaction.id] = transaction

    return {
        "voucher": voucher,
        "transaction_id": transaction.id,
    }


@app.get("/vouchers")
async def list_petty_cash_vouchers(
    fund_id: Optional[str] = None,
    status: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None
):
    """List petty cash vouchers"""
    results = list(petty_cash_vouchers.values())

    if fund_id:
        results = [v for v in results if v.fund_id == fund_id]
    if status:
        results = [v for v in results if v.status == status]
    if start_date:
        results = [v for v in results if v.date >= start_date]
    if end_date:
        results = [v for v in results if v.date <= end_date]

    results.sort(key=lambda x: x.date, reverse=True)
    return results


@app.put("/vouchers/{voucher_id}/approve")
async def approve_voucher(voucher_id: str, approved_by: str):
    """Approve a voucher"""
    if voucher_id not in petty_cash_vouchers:
        raise HTTPException(status_code=404, detail="Voucher not found")

    voucher = petty_cash_vouchers[voucher_id]
    voucher.approved_by = approved_by
    voucher.status = "approved"

    return voucher


# --- Replenishment ---

@app.post("/replenishments")
async def create_replenishment(replenishment: PettyCashReplenishment):
    """Create replenishment request"""
    replenishment.id = str(uuid.uuid4())
    replenishment.request_date = datetime.now(timezone.utc)

    petty_cash_replenishments[replenishment.id] = replenishment

    # Update fund status
    if replenishment.fund_id in petty_cash_funds:
        fund = petty_cash_funds[replenishment.fund_id]
        fund.status = PettyCashStatus.REPLENISHING
        fund.updated_at = datetime.now(timezone.utc)

    return replenishment


@app.get("/replenishments")
async def list_replenishments(
    fund_id: Optional[str] = None,
    status: Optional[ReimbursementStatus] = None
):
    """List replenishment requests"""
    results = list(petty_cash_replenishments.values())

    if fund_id:
        results = [r for r in results if r.fund_id == fund_id]
    if status:
        results = [r for r in results if r.status == status]

    return results


@app.put("/replenishments/{replenishment_id}/approve")
async def approve_replenishment(
    replenishment_id: str,
    approved_by: str
):
    """Approve replenishment"""
    if replenishment_id not in petty_cash_replenishments:
        raise HTTPException(status_code=404, detail="Replenishment not found")

    repl = petty_cash_replenishments[replenishment_id]
    repl.status = ReimbursementStatus.APPROVED
    repl.approved_by = approved_by
    repl.approved_date = datetime.now(timezone.utc)

    # Create replenishment transaction
    transaction = PettyCashTransaction(
        id=str(uuid.uuid4()),
        fund_id=repl.fund_id,
        transaction_type=TransactionType.REPLENISHMENT,
        amount=repl.amount,
        date=datetime.now(timezone.utc),
        description=f"Replenishment #{repl.id}",
        category=PaymentCategory.MISCELLANEOUS,
        approved_by=approved_by,
        entered_by=repl.requested_by,
    )
    petty_cash_transactions[transaction.id] = transaction

    # Update fund status
    if repl.fund_id in petty_cash_funds:
        fund = petty_cash_funds[repl.fund_id]
        fund.status = PettyCashStatus.ACTIVE
        fund.updated_at = datetime.now(timezone.utc)

    return {
        "replenishment": repl,
        "transaction_id": transaction.id,
    }


# --- Reports ---

@app.get("/reports/fund-report/{fund_id}")
async def get_fund_report(
    fund_id: str,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None
):
    """Generate fund report"""
    if fund_id not in petty_cash_funds:
        raise HTTPException(status_code=404, detail="Fund not found")

    fund = petty_cash_funds[fund_id]
    transactions = [t for t in petty_cash_transactions.values() if t.fund_id == fund_id]

    if start_date:
        transactions = [t for t in transactions if t.date >= start_date]
    if end_date:
        transactions = [t for t in transactions if t.date <= end_date]

    by_category = {}
    for t in transactions:
        cat = t.category.value
        if cat not in by_category:
            by_category[cat] = {"count": 0, "total": Decimal("0")}
        by_category[cat]["count"] += 1
        by_category[cat]["total"] += t.amount

    return {
        "fund": fund.model_dump(),
        "period": {"start": start_date, "end": end_date},
        "total_transactions": len(transactions),
        "by_category": by_category,
        "balance_info": await get_fund_balance(fund_id),
    }


@app.get("/reports/cash-position")
async def get_cash_position():
    """Get cash position across all funds"""
    funds_summary = []

    for fund_id, fund in petty_cash_funds.items():
        balance_info = await get_fund_balance(fund_id)
        current_balance = Decimal(balance_info["current_balance"])

        funds_summary.append({
            "fund_id": fund_id,
            "fund_name": fund.fund_name,
            "location": fund.location,
            "custodian": fund.custodian_name,
            "current_balance": str(current_balance),
            "maximum_balance": str(fund.maximum_balance),
            "utilization_percentage": float(current_balance / fund.maximum_balance * 100),
            "status": fund.status.value,
        })

    total_balance = sum(Decimal(f["current_balance"]) for f in funds_summary)

    return {
        "total_funds": len(funds_summary),
        "total_cash": str(total_balance),
        "funds": funds_summary,
    }


@app.get("/reports/category-summary")
async def get_category_summary(
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None
):
    """Get summary by payment category"""
    transactions = list(petty_cash_transactions.values())

    if start_date:
        transactions = [t for t in transactions if t.date >= start_date]
    if end_date:
        transactions = [t for t in transactions if t.date <= end_date]

    category_summary = {}
    for t in transactions:
        if t.transaction_type == TransactionType.PAYMENT:
            cat = t.category.value
            if cat not in category_summary:
                category_summary[cat] = {"count": 0, "total": Decimal("0")}
            category_summary[cat]["count"] += 1
            category_summary[cat]["total"] += t.amount

    return {
        "categories": category_summary,
        "total_transactions": sum(c["count"] for c in category_summary.values()),
        "total_amount": sum(c["total"] for c in category_summary.values()),
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8097)