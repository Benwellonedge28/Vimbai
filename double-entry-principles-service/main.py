"""
Vimbai Double Entry Principles Service
Core double entry bookkeeping rules, debit/credit logic, and account type behaviors.
"""

import os
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "double-entry-principles-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8030"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Double Entry Principles Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class AccountCategory(str, Enum):
    ASSET = "asset"
    LIABILITY = "liability"
    EQUITY = "equity"
    REVENUE = "revenue"
    EXPENSE = "expense"


class AccountType(str, Enum):
    CURRENT_ASSET = "current_asset"
    NON_CURRENT_ASSET = "non_current_asset"
    FIXED_ASSET = "fixed_asset"
    INTANGIBLE_ASSET = "intangible_asset"
    CURRENT_LIABILITY = "current_liability"
    NON_CURRENT_LIABILITY = "non_current_liability"
    LONG_TERM_LIABILITY = "long_term_liability"
    EQUITY = "equity"
    CAPITAL = "capital"
    REVENUE = "revenue"
    OTHER_INCOME = "other_income"
    DIRECT_EXPENSE = "direct_expense"
    INDIRECT_EXPENSE = "indirect_expense"
    ADMIN_EXPENSE = "administrative_expense"
    SELLING_EXPENSE = "selling_expense"
    FINANCIAL_EXPENSE = "financial_expense"


class EntryType(str, Enum):
    DEBIT = "debit"
    CREDIT = "credit"


class JournalEntryLine(BaseModel):
    account_code: str
    account_name: str
    debit: float = 0
    credit: float = 0
    description: Optional[str] = None


class JournalEntryRequest(BaseModel):
    date: datetime
    description: str
    entries: List[JournalEntryLine]
    reference: Optional[str] = None
    source_document: Optional[str] = None


class AccountBalance(BaseModel):
    account_code: str
    account_name: str
    account_type: AccountType
    category: AccountCategory
    debit_balance: float = 0
    credit_balance: float = 0
    net_balance: float = 0
    normal_balance: EntryType


class TransactionRule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    transaction_type: str
    account_type: AccountType
    debit_or_credit: EntryType
    explanation: str
    examples: List[str] = []


class DoubleEntryRule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    rule_name: str
    debit_accounts: List[AccountType]
    credit_accounts: List[AccountType]
    description: str
    journal_entry_template: Dict[str, Any] = {}


# In-memory rules storage
transaction_rules: List[TransactionRule] = []
double_entry_rules: Dict[str, DoubleEntryRule] = {}


def get_normal_balance(account_type: AccountType) -> EntryType:
    """Get normal balance direction for an account type."""
    debit_normal = [
        AccountType.CURRENT_ASSET, AccountType.NON_CURRENT_ASSET, AccountType.FIXED_ASSET,
        AccountType.INTANGIBLE_ASSET, AccountType.DIRECT_EXPENSE, AccountType.INDIRECT_EXPENSE,
        AccountType.ADMIN_EXPENSE, AccountType.SELLING_EXPENSE, AccountType.FINANCIAL_EXPENSE,
    ]
    return EntryType.DEBIT if account_type in debit_normal else EntryType.CREDIT


def get_account_category(account_type: AccountType) -> AccountCategory:
    """Map account type to category."""
    if account_type in [AccountType.CURRENT_ASSET, AccountType.NON_CURRENT_ASSET, AccountType.FIXED_ASSET, AccountType.INTANGIBLE_ASSET]:
        return AccountCategory.ASSET
    elif account_type in [AccountType.CURRENT_LIABILITY, AccountType.NON_CURRENT_LIABILITY, AccountType.LONG_TERM_LIABILITY]:
        return AccountCategory.LIABILITY
    elif account_type in [AccountType.EQUITY, AccountType.CAPITAL]:
        return AccountCategory.EQUITY
    elif account_type in [AccountType.REVENUE, AccountType.OTHER_INCOME]:
        return AccountCategory.REVENUE
    else:
        return AccountCategory.EXPENSE


def validate_double_entry(entries: List[JournalEntryLine]) -> tuple[bool, float, float]:
    """Validate that debits equal credits."""
    total_debit = sum(e.debit for e in entries)
    total_credit = sum(e.credit for e in entries)
    is_valid = abs(total_debit - total_credit) < 0.01  # Allow for floating point
    return is_valid, total_debit, total_credit


def get_debit_credit_for_transaction(
    account_type: AccountType, transaction_type: str, amount: float
) -> tuple[float, float]:
    """
    Determine debit/credit amounts based on transaction and account type.
    Returns (debit, credit).
    """
    normal_balance = get_normal_balance(account_type)

    increase_debit = [
        AccountType.CURRENT_ASSET, AccountType.NON_CURRENT_ASSET, AccountType.FIXED_ASSET,
        AccountType.INTANGIBLE_ASSET, AccountType.DIRECT_EXPENSE, AccountType.INDIRECT_EXPENSE,
        AccountType.ADMIN_EXPENSE, AccountType.SELLING_EXPENSE, AccountType.FINANCIAL_EXPENSE,
    ]

    if account_type in increase_debit:
        if transaction_type in ["increase", "debit_transaction"]:
            return amount, 0
        else:  # decrease
            return 0, amount
    else:  # Credit normal balance
        if transaction_type in ["increase", "credit_transaction"]:
            return 0, amount
        else:  # decrease
            return amount, 0


def get_account_balance_type(account_type: AccountType, debit_amount: float, credit_amount: float) -> str:
    """Determine if account balance is debit or credit."""
    normal = get_normal_balance(account_type)
    net = debit_amount - credit_amount

    if normal == EntryType.DEBIT:
        return "Debit Balance" if net >= 0 else "Credit Balance"
    else:
        return "Credit Balance" if net >= 0 else "Debit Balance"


# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy", "timestamp": datetime.utcnow().isoformat()}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Double entry bookkeeping principles and rules"}


@app.post("/validate-journal-entry")
async def validate_journal_entry(entry: JournalEntryRequest):
    """Validate a journal entry follows double entry rules."""
    is_valid, total_debit, total_credit = validate_double_entry(entry.entries)

    if not is_valid:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST,
            detail=f"Double entry violation: Debits ({total_debit}) must equal Credits ({total_credit})")

    return {
        "valid": True,
        "total_debit": total_debit,
        "total_credit": total_credit,
        "difference": abs(total_debit - total_credit),
        "message": "Journal entry is valid"
    }


@app.post("/calculate-entry")
async def calculate_double_entry(
    transaction_type: str,
    amount: float,
    account_type: AccountType,
    is_increase: bool = True
):
    """Calculate debit/credit for a transaction."""
    debit, credit = get_debit_credit_for_transaction(
        account_type,
        "increase" if is_increase else "decrease",
        amount
    )

    return {
        "account_type": account_type,
        "transaction_type": transaction_type,
        "is_increase": is_increase,
        "debit": debit,
        "credit": credit,
        "normal_balance": get_normal_balance(account_type),
        "category": get_account_category(account_type)
    }


@app.get("/account-rules/{account_type}")
async def get_account_rules(account_type: AccountType):
    """Get debit/credit rules for an account type."""
    normal = get_normal_balance(account_type)
    category = get_account_category(account_type)

    rules = {
        AccountType.CURRENT_ASSET: ["Increases with Debit", "Decreases with Credit", "Normal: Debit"],
        AccountType.NON_CURRENT_ASSET: ["Increases with Debit", "Decreases with Credit", "Normal: Debit"],
        AccountType.FIXED_ASSET: ["Increases with Debit", "Decreases with Credit", "Normal: Debit"],
        AccountType.CURRENT_LIABILITY: ["Increases with Credit", "Decreases with Debit", "Normal: Credit"],
        AccountType.NON_CURRENT_LIABILITY: ["Increases with Credit", "Decreases with Debit", "Normal: Credit"],
        AccountType.EQUITY: ["Increases with Credit", "Decreases with Debit", "Normal: Credit"],
        AccountType.CAPITAL: ["Increases with Credit", "Decreases with Debit", "Normal: Credit"],
        AccountType.REVENUE: ["Increases with Credit", "Decreases with Debit", "Normal: Credit"],
        AccountType.OTHER_INCOME: ["Increases with Credit", "Decreases with Debit", "Normal: Credit"],
        AccountType.DIRECT_EXPENSE: ["Increases with Debit", "Decreases with Credit", "Normal: Debit"],
        AccountType.INDIRECT_EXPENSE: ["Increases with Debit", "Decreases with Credit", "Normal: Debit"],
        AccountType.ADMIN_EXPENSE: ["Increases with Debit", "Decreases with Credit", "Normal: Debit"],
        AccountType.SELLING_EXPENSE: ["Increases with Debit", "Decreases with Credit", "Normal: Debit"],
        AccountType.FINANCIAL_EXPENSE: ["Increases with Debit", "Decreases with Credit", "Normal: Debit"],
    }

    return {
        "account_type": account_type,
        "category": category,
        "normal_balance": normal,
        "rules": rules.get(account_type, ["Default rules apply"]),
        "examples": _get_transaction_examples(account_type)
    }


def _get_transaction_examples(account_type: AccountType) -> List[Dict[str, Any]]:
    """Get common transaction examples for account type."""
    examples = {
        AccountType.CASH: [{"transaction": "Receive cash", "debit": True}, {"transaction": "Pay cash", "debit": False}],
        AccountType.CURRENT_ASSET: [{"transaction": "Purchase asset", "debit": True}, {"transaction": "Sell asset", "debit": False}],
        AccountType.CURRENT_LIABILITY: [{"transaction": "Incur liability", "debit": False}, {"transaction": "Pay liability", "debit": True}],
        AccountType.REVENUE: [{"transaction": "Make sale", "debit": False}, {"transaction": "Receive income", "debit": False}],
        AccountType.EXPENSE: [{"transaction": "Pay expense", "debit": True}, {"transaction": "Expense incurred", "debit": True}],
    }
    return examples.get(account_type, [])


@app.get("/account-types")
async def get_all_account_types():
    """Get all account types with their categories and normal balances."""
    return {
        "account_types": [
            {"type": at.value, "category": get_account_category(at), "normal_balance": get_normal_balance(at)}
            for at in AccountType
        ]
    }


@app.post("/journal-entries/{entry_type}")
async def get_journal_entry_template(entry_type: str):
    """Get standard journal entry templates for common transactions."""
    templates = {
        "sale": {
            "description": "Recording a sale",
            "entries": [
                {"account_type": "CURRENT_ASSET", "debit": True, "credit": False, "explanation": "Debit Customers/Receivables"},
                {"account_type": "REVENUE", "debit": False, "credit": True, "explanation": "Credit Sales Revenue"},
            ]
        },
        "purchase": {
            "description": "Recording a purchase",
            "entries": [
                {"account_type": "EXPENSE", "debit": True, "credit": False, "explanation": "Debit Purchases/Expense"},
                {"account_type": "CURRENT_LIABILITY", "debit": False, "credit": True, "explanation": "Credit Suppliers/Payables"},
            ]
        },
        "cash_receipt": {
            "description": "Receiving cash",
            "entries": [
                {"account_type": "CURRENT_ASSET", "debit": True, "credit": False, "explanation": "Debit Cash/Bank"},
                {"account_type": "REVENUE", "debit": False, "credit": True, "explanation": "Credit Revenue"},
            ]
        },
        "cash_payment": {
            "description": "Paying cash",
            "entries": [
                {"account_type": "EXPENSE", "debit": True, "credit": False, "explanation": "Debit Expense"},
                {"account_type": "CURRENT_ASSET", "debit": False, "credit": True, "explanation": "Credit Cash/Bank"},
            ]
        },
        "depreciation": {
            "description": "Recording depreciation",
            "entries": [
                {"account_type": "INDIRECT_EXPENSE", "debit": True, "credit": False, "explanation": "Debit Depreciation Expense"},
                {"account_type": "NON_CURRENT_ASSET", "debit": False, "credit": True, "explanation": "Credit Accumulated Depreciation"},
            ]
        },
        "bad_debt": {
            "description": "Writing off bad debt",
            "entries": [
                {"account_type": "INDIRECT_EXPENSE", "debit": True, "credit": False, "explanation": "Debit Bad Debts Expense"},
                {"account_type": "CURRENT_ASSET", "debit": False, "credit": True, "explanation": "Credit Accounts Receivable"},
            ]
        },
        "provision_doubtful_debt": {
            "description": "Creating provision for doubtful debts",
            "entries": [
                {"account_type": "INDIRECT_EXPENSE", "debit": True, "credit": False, "explanation": "Debit Bad Debts Expense"},
                {"account_type": "CURRENT_ASSET", "debit": False, "credit": True, "explanation": "Credit Provision for Doubtful Debts"},
            ]
        },
    }

    if entry_type not in templates:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Template for {entry_type} not found")

    return templates[entry_type]


@app.post("/transaction-analyzer")
async def analyze_transaction(transaction_description: str, amounts: Dict[str, float]):
    """Analyze a transaction and suggest the correct journal entry."""
    desc_lower = transaction_description.lower()

    # Common transaction patterns
    if "sale" in desc_lower or "revenue" in desc_lower:
        amount = amounts.get("amount", 0)
        return {
            "transaction": transaction_description,
            "suggested_entry": {
                "description": f"Sale: {transaction_description}",
                "entries": [
                    {"account_type": "CURRENT_ASSET", "account_name": "Accounts Receivable", "debit": amount, "credit": 0},
                    {"account_type": "REVENUE", "account_name": "Sales Revenue", "debit": 0, "credit": amount},
                ]
            }
        }
    elif "purchase" in desc_lower and "asset" not in desc_lower:
        amount = amounts.get("amount", 0)
        return {
            "transaction": transaction_description,
            "suggested_entry": {
                "description": f"Purchase: {transaction_description}",
                "entries": [
                    {"account_type": "EXPENSE", "account_name": "Purchases", "debit": amount, "credit": 0},
                    {"account_type": "CURRENT_LIABILITY", "account_name": "Accounts Payable", "debit": 0, "credit": amount},
                ]
            }
        }
    elif "depreciation" in desc_lower:
        amount = amounts.get("amount", 0)
        return {
            "transaction": transaction_description,
            "suggested_entry": {
                "description": f"Depreciation: {transaction_description}",
                "entries": [
                    {"account_type": "INDIRECT_EXPENSE", "account_name": "Depreciation Expense", "debit": amount, "credit": 0},
                    {"account_type": "NON_CURRENT_ASSET", "account_name": "Accumulated Depreciation", "debit": 0, "credit": amount},
                ]
            }
        }
    else:
        return {
            "transaction": transaction_description,
            "message": "Transaction type not recognized. Please provide more details.",
            "amounts": amounts
        }


@app.get("/accounting-equation")
async def get_accounting_equation():
    """Get the fundamental accounting equation."""
    return {
        "equation": "Assets = Liabilities + Equity",
        "explanation": "The fundamental accounting equation states that total assets equal total liabilities plus total equity.",
        "elements": {
            "assets": {"definition": "Resources owned by the business", "normal_balance": "Debit"},
            "liabilities": {"definition": "Amounts owed to others", "normal_balance": "Credit"},
            "equity": {"definition": "Owner's stake in the business", "normal_balance": "Credit"},
        },
        "rules": [
            "Every transaction affects at least two accounts",
            "Total debits must equal total credits",
            "The equation must always balance"
        ]
    }


if __name__ == "__main__":
    import uvicorn
    logger.info("starting_double_entry_principles_service", port=PORT)
    uvicorn.run(app, host="0.0.0.0", port=PORT)