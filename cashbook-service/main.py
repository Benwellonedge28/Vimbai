"""
Vimbai Cash Book Service
Complete cash book management with multi-currency support,
bank reconciliation, and cash flow tracking
"""

import uuid
from datetime import datetime, timezone
from decimal import Decimal
from enum import Enum
from typing import Any, Dict, List, Optional

from fastapi import Depends, FastAPI, HTTPException, Query
from pydantic import BaseModel, Field

app = FastAPI(
    title="Vimbai Cash Book Service",
    description="Comprehensive cash book management with multi-currency support, bank reconciliation, and cash flow tracking",
    version="1.0.0",
)

# ============================================================================
# Enums
# ============================================================================


class CashBookType(str, Enum):
    RECEIPTS = "receipts"
    PAYMENTS = "payments"
    CASH_JOURNAL = "cash_journal"


class TransactionStatus(str, Enum):
    PENDING = "pending"
    POSTED = "posted"
    RECONCILED = "reconciled"
    VOIDED = "voided"


class BankAccountType(str, Enum):
    CASH = "cash"
    BANK = "bank"
    SAVINGS = "savings"
    CURRENT = "current"


# ============================================================================
# Pydantic Models
# ============================================================================


class BankAccount(BaseModel):
    id: str
    account_code: str
    account_name: str
    account_type: BankAccountType
    bank_name: Optional[str] = None
    account_number: Optional[str] = None
    currency: str = "USD"
    opening_balance: Decimal
    current_balance: Decimal
    is_active: bool = True
    reconciliation_enabled: bool = True
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashBookEntry(BaseModel):
    id: str
    book_type: CashBookType
    entry_date: datetime
    voucher_number: str
    description: str
    account_code: str
    amount: Decimal
    is_debit: bool  # True for receipts/cash in, False for payments/cash out
    reference: Optional[str] = None
    cheque_number: Optional[str] = None
    bank_account: Optional[str] = None
    currency: str = "USD"
    exchange_rate: Decimal = Decimal("1.0")
    base_amount: Decimal  # Amount in base currency
    narration: Optional[str] = None
    posted_by: str
    status: TransactionStatus = TransactionStatus.PENDING
    reconciled: bool = False
    reconciliation_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashBookSummary(BaseModel):
    account_code: str
    account_name: str
    opening_balance: Decimal
    total_debits: Decimal
    total_credits: Decimal
    closing_balance: Decimal
    transaction_count: int
    last_transaction_date: Optional[datetime] = None


class BankReconciliation(BaseModel):
    id: str
    bank_account: str
    statement_date: datetime
    statement_balance: Decimal
    book_balance: Decimal
    adjustments: List[Dict[str, Any]] = []
    adjusted_balance: Decimal
    differences: List[Dict[str, Any]] = []
    status: str = "in_progress"  # in_progress, completed, reviewed
    prepared_by: str
    reviewed_by: Optional[str] = None
    completed_at: Optional[datetime] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashFlowEntry(BaseModel):
    id: str
    entry_date: datetime
    category: str
    subcategory: Optional[str] = None
    description: str
    expected_amount: Optional[Decimal] = None
    actual_amount: Optional[Decimal] = None
    variance: Optional[Decimal] = None
    cash_flow_type: str  # operating, investing, financing
    source: str  # cash_receipts, cash_payments, bank
    reference_id: Optional[str] = None
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class CashPosition(BaseModel):
    as_of_date: datetime
    total_cash: Decimal
    total_bank: Decimal
    total_savings: Decimal
    total_current: Decimal
    by_currency: Dict[str, Decimal]
    by_account: List[Dict[str, Any]]


# ============================================================================
# Storage
# ============================================================================

bank_accounts: Dict[str, BankAccount] = {}
cash_book_entries: Dict[str, CashBookEntry] = {}
reconciliations: Dict[str, BankReconciliation] = {}
cash_flow_entries: Dict[str, CashFlowEntry] = {}


# ============================================================================
# API Endpoints
# ============================================================================


@app.get("/")
async def health_check():
    """Health check endpoint"""
    total_accounts = len(bank_accounts)
    total_entries = len(cash_book_entries)

    return {
        "status": "healthy",
        "service": "cashbook",
        "version": "1.0.0",
        "total_accounts": total_accounts,
        "total_entries": total_entries,
    }


# --- Bank Account Management ---


@app.post("/accounts")
async def create_bank_account(account: BankAccount):
    """Create a new bank/cash account"""
    account.id = str(uuid.uuid4())
    account.created_at = datetime.now(timezone.utc)
    account.current_balance = account.opening_balance

    bank_accounts[account.id] = account
    return account


@app.get("/accounts")
async def list_bank_accounts(
    account_type: Optional[BankAccountType] = None, currency: Optional[str] = None, is_active: Optional[bool] = None
):
    """List all bank accounts"""
    results = list(bank_accounts.values())

    if account_type:
        results = [a for a in results if a.account_type == account_type]
    if currency:
        results = [a for a in results if a.currency == currency]
    if is_active is not None:
        results = [a for a in results if a.is_active == is_active]

    return results


@app.get("/accounts/{account_id}")
async def get_bank_account(account_id: str):
    """Get bank account details"""
    if account_id not in bank_accounts:
        raise HTTPException(status_code=404, detail="Account not found")
    return bank_accounts[account_id]


@app.put("/accounts/{account_id}")
async def update_bank_account(account_id: str, account: BankAccount):
    """Update bank account"""
    if account_id not in bank_accounts:
        raise HTTPException(status_code=404, detail="Account not found")

    account.id = account_id
    bank_accounts[account_id] = account
    return account


# --- Cash Book Entry Management ---


@app.post("/entries")
async def create_cash_book_entry(entry: CashBookEntry):
    """Create a cash book entry"""
    entry.id = str(uuid.uuid4())
    entry.created_at = datetime.now(timezone.utc)

    # Calculate base amount if multi-currency
    if entry.currency != "USD":
        entry.base_amount = entry.amount * entry.exchange_rate
    else:
        entry.base_amount = entry.amount

    # Update bank account balance
    if entry.bank_account:
        for account in bank_accounts.values():
            if account.account_code == entry.bank_account:
                if entry.is_debit:
                    account.current_balance += entry.base_amount
                else:
                    account.current_balance -= entry.base_amount
                break

    cash_book_entries[entry.id] = entry
    return entry


@app.get("/entries")
async def list_cash_book_entries(
    book_type: Optional[CashBookType] = None,
    bank_account: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    status: Optional[TransactionStatus] = None,
    reconciled: Optional[bool] = None,
    limit: int = 100,
):
    """List cash book entries with filters"""
    results = list(cash_book_entries.values())

    if book_type:
        results = [e for e in results if e.book_type == book_type]
    if bank_account:
        results = [e for e in results if e.bank_account == bank_account]
    if start_date:
        results = [e for e in results if e.entry_date >= start_date]
    if end_date:
        results = [e for e in results if e.entry_date <= end_date]
    if status:
        results = [e for e in results if e.status == status]
    if reconciled is not None:
        results = [e for e in results if e.reconciled == reconciled]

    results.sort(key=lambda x: x.entry_date, reverse=True)
    return results[:limit]


@app.get("/entries/{entry_id}")
async def get_cash_book_entry(entry_id: str):
    """Get cash book entry details"""
    if entry_id not in cash_book_entries:
        raise HTTPException(status_code=404, detail="Entry not found")
    return cash_book_entries[entry_id]


@app.put("/entries/{entry_id}/post")
async def post_cash_book_entry(entry_id: str, posted_by: str):
    """Post a cash book entry"""
    if entry_id not in cash_book_entries:
        raise HTTPException(status_code=404, detail="Entry not found")

    entry = cash_book_entries[entry_id]
    entry.status = TransactionStatus.POSTED

    return entry


@app.put("/entries/{entry_id}/void")
async def void_cash_book_entry(entry_id: str, voided_by: str, reason: str):
    """Void a cash book entry"""
    if entry_id not in cash_book_entries:
        raise HTTPException(status_code=404, detail="Entry not found")

    entry = cash_book_entries[entry_id]
    entry.status = TransactionStatus.VOIDED

    # Reverse the balance change
    if entry.bank_account:
        for account in bank_accounts.values():
            if account.account_code == entry.bank_account:
                if entry.is_debit:
                    account.current_balance -= entry.base_amount
                else:
                    account.current_balance += entry.base_amount
                break

    return {"status": "voided", "entry_id": entry_id, "reason": reason}


# --- Cash Book Summary ---


@app.get("/summary/{account_code}")
async def get_cash_book_summary(
    account_code: str, start_date: Optional[datetime] = None, end_date: Optional[datetime] = None
):
    """Get cash book summary for an account"""
    if account_code not in [a.account_code for a in bank_accounts.values()]:
        raise HTTPException(status_code=404, detail="Account not found")

    account = next(a for a in bank_accounts.values() if a.account_code == account_code)
    entries = [e for e in cash_book_entries.values() if e.bank_account == account_code]

    if start_date:
        entries = [e for e in entries if e.entry_date >= start_date]
    if end_date:
        entries = [e for e in entries if e.entry_date <= end_date]

    total_debits = sum(e.base_amount for e in entries if e.is_debit and e.status == TransactionStatus.POSTED)
    total_credits = sum(e.base_amount for e in entries if not e.is_debit and e.status == TransactionStatus.POSTED)

    last_transaction = max((e.entry_date for e in entries), default=None)

    return CashBookSummary(
        account_code=account_code,
        account_name=account.account_name,
        opening_balance=account.opening_balance,
        total_debits=total_debits,
        total_credits=total_credits,
        closing_balance=account.opening_balance + total_debits - total_credits,
        transaction_count=len(entries),
        last_transaction_date=last_transaction,
    )


# --- Bank Reconciliation ---


@app.post("/reconciliations")
async def create_reconciliation(reconciliation: BankReconciliation):
    """Create bank reconciliation"""
    reconciliation.id = str(uuid.uuid4())
    reconciliation.created_at = datetime.now(timezone.utc)

    # Calculate book balance
    entries = [
        e
        for e in cash_book_entries.values()
        if e.bank_account == reconciliation.bank_account and e.status == TransactionStatus.POSTED
    ]
    book_balance = sum(e.base_amount if e.is_debit else -e.base_amount for e in entries)

    # Get account opening balance
    account = next((a for a in bank_accounts.values() if a.account_code == reconciliation.bank_account), None)
    if account:
        book_balance = account.opening_balance + book_balance

    reconciliation.book_balance = book_balance

    # Calculate adjusted balance
    total_adjustments = sum(a.get("amount", 0) for a in reconciliation.adjustments)
    reconciliation.adjusted_balance = reconciliation.statement_balance + total_adjustments

    reconciliations[reconciliation.id] = reconciliation

    return reconciliation


@app.get("/reconciliations")
async def list_reconciliations(bank_account: Optional[str] = None, status: Optional[str] = None):
    """List bank reconciliations"""
    results = list(reconciliations.values())

    if bank_account:
        results = [r for r in results if r.bank_account == bank_account]
    if status:
        results = [r for r in results if r.status == status]

    results.sort(key=lambda x: x.statement_date, reverse=True)
    return results


@app.get("/reconciliations/{reconciliation_id}")
async def get_reconciliation(reconciliation_id: str):
    """Get reconciliation details"""
    if reconciliation_id not in reconciliations:
        raise HTTPException(status_code=404, detail="Reconciliation not found")
    return reconciliations[reconciliation_id]


@app.put("/reconciliations/{reconciliation_id}/complete")
async def complete_reconciliation(reconciliation_id: str, reviewed_by: str):
    """Complete a reconciliation"""
    if reconciliation_id not in reconciliations:
        raise HTTPException(status_code=404, detail="Reconciliation not found")

    reconciliation = reconciliations[reconciliation_id]
    reconciliation.status = "completed"
    reconciliation.reviewed_by = reviewed_by
    reconciliation.completed_at = datetime.now(timezone.utc)

    # Mark entries as reconciled
    for adjustment in reconciliation.adjustments:
        if "entry_id" in adjustment:
            entry = cash_book_entries.get(adjustment["entry_id"])
            if entry:
                entry.reconciled = True
                entry.reconciliation_id = reconciliation_id

    return reconciliation


@app.get("/reconciliations/{reconciliation_id}/outstanding")
async def get_outstanding_items(reconciliation_id: str):
    """Get outstanding items for reconciliation"""
    if reconciliation_id not in reconciliations:
        raise HTTPException(status_code=404, detail="Reconciliation not found")

    reconciliation = reconciliations[reconciliation_id]

    # Get unreconciled entries
    entries = [
        e
        for e in cash_book_entries.values()
        if e.bank_account == reconciliation.bank_account and not e.reconciled and e.status == TransactionStatus.POSTED
    ]

    total_debits = sum(e.base_amount for e in entries if e.is_debit)
    total_credits = sum(e.base_amount for e in entries if not e.is_debit)

    return {
        "reconciliation_id": reconciliation_id,
        "total_items": len(entries),
        "total_debits": str(total_debits),
        "total_credits": str(total_credits),
        "difference": str(total_debits - total_credits),
        "entries": [e.model_dump() for e in entries],
    }


# --- Cash Flow ---


@app.post("/cash-flow")
async def create_cash_flow_entry(entry: CashFlowEntry):
    """Create cash flow entry"""
    entry.id = str(uuid.uuid4())
    entry.created_at = datetime.now(timezone.utc)

    # Calculate variance if actual amount provided
    if entry.expected_amount and entry.actual_amount:
        entry.variance = entry.actual_amount - entry.expected_amount

    cash_flow_entries[entry.id] = entry
    return entry


@app.get("/cash-flow")
async def list_cash_flow_entries(
    cash_flow_type: Optional[str] = None,
    category: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
):
    """List cash flow entries"""
    results = list(cash_flow_entries.values())

    if cash_flow_type:
        results = [e for e in results if e.cash_flow_type == cash_flow_type]
    if category:
        results = [e for e in results if e.category == category]
    if start_date:
        results = [e for e in results if e.entry_date >= start_date]
    if end_date:
        results = [e for e in results if e.entry_date <= end_date]

    results.sort(key=lambda x: x.entry_date, reverse=True)
    return results[:limit]


@app.get("/cash-flow/summary")
async def get_cash_flow_summary(start_date: datetime, end_date: datetime):
    """Get cash flow summary for period"""
    entries = list(cash_flow_entries.values())
    entries = [e for e in entries if start_date <= e.entry_date <= end_date]

    operating = [e for e in entries if e.cash_flow_type == "operating"]
    investing = [e for e in entries if e.cash_flow_type == "investing"]
    financing = [e for e in entries if e.cash_flow_type == "financing"]

    def calculate_total(entries_list):
        actual = sum(e.actual_amount for e in entries_list if e.actual_amount)
        expected = sum(e.expected_amount for e in entries_list if e.expected_amount)
        return {
            "count": len(entries_list),
            "expected_total": str(expected),
            "actual_total": str(actual),
            "variance": str(expected - actual),
        }

    return {
        "period": {"start": start_date, "end": end_date},
        "operating": calculate_total(operating),
        "investing": calculate_total(investing),
        "financing": calculate_total(financing),
        "net_change": str(sum(e.actual_amount or 0 for e in entries) - sum(e.expected_amount or 0 for e in entries)),
    }


# --- Cash Position ---


@app.get("/cash-position")
async def get_cash_position(as_of_date: Optional[datetime] = None):
    """Get current cash position"""
    if as_of_date is None:
        as_of_date = datetime.now(timezone.utc)

    total_cash = Decimal("0")
    total_bank = Decimal("0")
    total_savings = Decimal("0")
    total_current = Decimal("0")
    by_currency: Dict[str, Decimal] = {}
    by_account = []

    for account in bank_accounts.values():
        balance = account.current_balance

        account_data = {
            "account_code": account.account_code,
            "account_name": account.account_name,
            "account_type": account.account_type.value,
            "balance": str(balance),
            "currency": account.currency,
        }
        by_account.append(account_data)

        # Accumulate by type
        if account.account_type == BankAccountType.CASH:
            total_cash += balance
        elif account.account_type == BankAccountType.BANK:
            total_bank += balance
        elif account.account_type == BankAccountType.SAVINGS:
            total_savings += balance
        elif account.account_type == BankAccountType.CURRENT:
            total_current += balance

        # Accumulate by currency
        if account.currency not in by_currency:
            by_currency[account.currency] = Decimal("0")
        by_currency[account.currency] += balance

    return CashPosition(
        as_of_date=as_of_date,
        total_cash=total_cash,
        total_bank=total_bank,
        total_savings=total_savings,
        total_current=total_current,
        by_currency={k: str(v) for k, v in by_currency.items()},
        by_account=by_account,
    )


@app.get("/cash-position/history")
async def get_cash_position_history(start_date: datetime, end_date: datetime):
    """Get cash position history"""
    entries = list(cash_book_entries.values())
    entries = [e for e in entries if start_date <= e.entry_date <= end_date]

    daily_positions = {}
    for entry in entries:
        date_key = entry.entry_date.date().isoformat()
        if date_key not in daily_positions:
            daily_positions[date_key] = {"debits": Decimal("0"), "credits": Decimal("0")}

        if entry.is_debit:
            daily_positions[date_key]["debits"] += entry.base_amount
        else:
            daily_positions[date_key]["credits"] += entry.base_amount

    return {
        "period": {"start": start_date, "end": end_date},
        "daily_positions": [
            {"date": k, "debits": str(v["debits"]), "credits": str(v["credits"])}
            for k, v in sorted(daily_positions.items())
        ],
    }


# --- Reports ---


@app.get("/reports/cash-receipts-register")
async def get_cash_receipts_register(
    start_date: Optional[datetime] = None, end_date: Optional[datetime] = None, bank_account: Optional[str] = None
):
    """Generate cash receipts register"""
    entries = list(cash_book_entries.values())
    entries = [e for e in entries if e.book_type == CashBookType.RECEIPTS]

    if start_date:
        entries = [e for e in entries if e.entry_date >= start_date]
    if end_date:
        entries = [e for e in entries if e.entry_date <= end_date]
    if bank_account:
        entries = [e for e in entries if e.bank_account == bank_account]

    entries.sort(key=lambda x: x.entry_date)

    total = sum(e.base_amount for e in entries if e.status == TransactionStatus.POSTED)

    return {
        "total_receipts": len(entries),
        "total_amount": str(total),
        "entries": [e.model_dump() for e in entries],
    }


@app.get("/reports/cash-payments-register")
async def get_cash_payments_register(
    start_date: Optional[datetime] = None, end_date: Optional[datetime] = None, bank_account: Optional[str] = None
):
    """Generate cash payments register"""
    entries = list(cash_book_entries.values())
    entries = [e for e in entries if e.book_type == CashBookType.PAYMENTS]

    if start_date:
        entries = [e for e in entries if e.entry_date >= start_date]
    if end_date:
        entries = [e for e in entries if e.entry_date <= end_date]
    if bank_account:
        entries = [e for e in entries if e.bank_account == bank_account]

    entries.sort(key=lambda x: x.entry_date)

    total = sum(e.base_amount for e in entries if e.status == TransactionStatus.POSTED)

    return {
        "total_payments": len(entries),
        "total_amount": str(total),
        "entries": [e.model_dump() for e in entries],
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8098)
