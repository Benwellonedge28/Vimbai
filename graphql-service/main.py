"""
Vimbai GraphQL API Service
Provides GraphQL interface for all Vimbai services
"""

from fastapi import FastAPI, Depends, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from strawberry.fastapi import GraphQLRouter
import strawberry
from typing import List, Optional, Dict, Any
from datetime import datetime
from decimal import Decimal
import asyncio
import os
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="Vimbai GraphQL API",
    description="GraphQL interface for Vimbai financial management system",
    version="0.1.0",
)

# CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# ============================================================================
# GraphQL Types
# ============================================================================

@strawberry.type
class AccountType:
    id: str
    name: str
    account_number: str
    account_type: str
    normal_balance: str
    balance: float
    is_active: bool
    created_at: datetime

@strawberry.type
class JournalEntryLine:
    account_number: str
    description: str
    debit: float
    credit: float

@strawberry.type
class JournalEntry:
    id: str
    entry_date: datetime
    description: str
    source_module: str
    lines: List[JournalEntryLine]
    total_debit: float
    total_credit: float
    is_balanced: bool
    created_by: str

@strawberry.type
class Transaction:
    id: str
    transaction_id: str
    amount: float
    transaction_type: str
    sender_account_id: str
    recipient_account_id: str
    timestamp: datetime
    status: str
    fraud_score: Optional[float] = None
    fraud_flag: Optional[str] = None

@strawberry.type
class FinancialStatement:
    id: str
    statement_type: str
    start_date: datetime
    end_date: datetime
    content: str
    generated_by: str

@strawberry.type
class Budget:
    id: str
    name: str
    category: str
    allocated_amount: float
    spent_amount: float
    remaining_amount: float
    period: str

@strawberry.type
class Alert:
    id: str
    title: str
    message: str
    severity: str
    category: str
    status: str
    created_at: datetime

@strawberry.type
class Notification:
    id: str
    type: str
    title: str
    message: str
    priority: str
    is_read: bool
    created_at: datetime

@strawberry.type
class Currency:
    code: str
    name: str
    symbol: str
    exchange_rate_to_usd: float

@strawberry.type
class ConversionResult:
    from_currency: str
    to_currency: str
    original_amount: float
    converted_amount: float
    rate_used: float

# ============================================================================
# GraphQL Queries
# ============================================================================

@strawberry.type
class Query:
    @strawberry.field
    async def accounts(
        self,
        account_type: Optional[str] = None,
        is_active: Optional[bool] = None
    ) -> List[AccountType]:
        """Get all accounts with optional filtering"""
        # In production, this would call the accounting service
        return [
            AccountType(
                id="acc-001",
                name="Cash Account",
                account_number="1001",
                account_type="asset",
                normal_balance="debit",
                balance=50000.0,
                is_active=True,
                created_at=datetime.now()
            ),
            AccountType(
                id="acc-002",
                name="Accounts Receivable",
                account_number="1100",
                account_type="asset",
                normal_balance="debit",
                balance=25000.0,
                is_active=True,
                created_at=datetime.now()
            )
        ]

    @strawberry.field
    async def account(self, id: str) -> Optional[AccountType]:
        """Get a specific account by ID"""
        return AccountType(
            id=id,
            name="Sample Account",
            account_number="1001",
            account_type="asset",
            normal_balance="debit",
            balance=50000.0,
            is_active=True,
            created_at=datetime.now()
        )

    @strawberry.field
    async def journal_entries(
        self,
        start_date: Optional[datetime] = None,
        end_date: Optional[datetime] = None,
        limit: int = 50
    ) -> List[JournalEntry]:
        """Get journal entries with optional date filtering"""
        return [
            JournalEntry(
                id="je-001",
                entry_date=datetime.now(),
                description="Sample journal entry",
                source_module="manual",
                lines=[
                    JournalEntryLine(account_number="1001", description="Cash", debit=1000.0, credit=0.0),
                    JournalEntryLine(account_number="4000", description="Revenue", debit=0.0, credit=1000.0)
                ],
                total_debit=1000.0,
                total_credit=1000.0,
                is_balanced=True,
                created_by="admin"
            )
        ]

    @strawberry.field
    async def journal_entry(self, id: str) -> Optional[JournalEntry]:
        """Get a specific journal entry"""
        return JournalEntry(
            id=id,
            entry_date=datetime.now(),
            description="Sample journal entry",
            source_module="manual",
            lines=[
                JournalEntryLine(account_number="1001", description="Cash", debit=1000.0, credit=0.0)
            ],
            total_debit=1000.0,
            total_credit=1000.0,
            is_balanced=True,
            created_by="admin"
        )

    @strawberry.field
    async def transactions(
        self,
        account_id: Optional[str] = None,
        start_date: Optional[datetime] = None,
        limit: int = 50
    ) -> List[Transaction]:
        """Get transactions with optional filtering"""
        return [
            Transaction(
                id="txn-001",
                transaction_id="TXN-2024-001",
                amount=1500.0,
                transaction_type="transfer",
                sender_account_id="acc-001",
                recipient_account_id="acc-002",
                timestamp=datetime.now(),
                status="completed",
                fraud_score=0.15,
                fraud_flag="safe"
            )
        ]

    @strawberry.field
    async def financial_statements(
        self,
        statement_type: str,
        start_date: datetime,
        end_date: datetime
    ) -> List[FinancialStatement]:
        """Get financial statements for a period"""
        return [
            FinancialStatement(
                id="fs-001",
                statement_type=statement_type,
                start_date=start_date,
                end_date=end_date,
                content="{}",
                generated_by="system"
            )
        ]

    @strawberry.field
    async def budgets(
        self,
        category: Optional[str] = None,
        is_active: Optional[bool] = None
    ) -> List[Budget]:
        """Get budgets with optional filtering"""
        return [
            Budget(
                id="bud-001",
                name="Marketing Budget 2024",
                category="marketing",
                allocated_amount=50000.0,
                spent_amount=25000.0,
                remaining_amount=25000.0,
                period="2024"
            )
        ]

    @strawberry.field
    async def alerts(
        self,
        status: Optional[str] = None,
        severity: Optional[str] = None,
        limit: int = 50
    ) -> List[Alert]:
        """Get alerts with optional filtering"""
        return [
            Alert(
                id="alert-001",
                title="High Value Transaction",
                message="Transaction of $15,000 detected",
                severity="high",
                category="fraud",
                status="active",
                created_at=datetime.now()
            )
        ]

    @strawberry.field
    async def notifications(
        self,
        user_id: str,
        unread_only: bool = False,
        limit: int = 50
    ) -> List[Notification]:
        """Get notifications for a user"""
        return [
            Notification(
                id="notif-001",
                type="approval_required",
                title="Approval Needed",
                message="Please approve expense report",
                priority="high",
                is_read=False,
                created_at=datetime.now()
            )
        ]

    @strawberry.field
    async def currencies(self) -> List[Currency]:
        """Get all supported currencies"""
        return [
            Currency(code="USD", name="US Dollar", symbol="$", exchange_rate_to_usd=1.0),
            Currency(code="EUR", name="Euro", symbol="€", exchange_rate_to_usd=0.92),
            Currency(code="GBP", name="British Pound", symbol="£", exchange_rate_to_usd=0.79)
        ]

    @strawberry.field
    async def exchange_rate(
        self,
        from_currency: str,
        to_currency: str
    ) -> Optional[float]:
        """Get exchange rate between two currencies"""
        rates = {
            ("USD", "EUR"): 0.92,
            ("USD", "GBP"): 0.79,
            ("EUR", "USD"): 1.09,
            ("GBP", "USD"): 1.27
        }
        return rates.get((from_currency.upper(), to_currency.upper()))

# ============================================================================
# GraphQL Mutations
# ============================================================================

@strawberry.input
class AccountInput:
    name: str
    account_number: str
    account_type: str
    normal_balance: str
    description: Optional[str] = None

@strawberry.input
class JournalEntryLineInput:
    account_number: str
    description: str
    debit: float
    credit: float

@strawberry.input
class JournalEntryInput:
    entry_date: datetime
    description: str
    source_module: str
    lines: List[JournalEntryLineInput]

@strawberry.input
class TransactionInput:
    amount: float
    transaction_type: str
    sender_account_id: str
    recipient_account_id: str

@strawberry.input
class CurrencyConversionInput:
    from_currency: str
    to_currency: str
    amount: float

@strawberry.type
class MutationResult:
    success: bool
    message: str
    id: Optional[str] = None

@strawberry.type
class Mutation:
    @strawberry.mutation
    async def create_account(self, input: AccountInput) -> MutationResult:
        """Create a new account"""
        # In production, this would call the accounting service
        return MutationResult(success=True, message="Account created", id="acc-new-001")

    @strawberry.mutation
    async def update_account(
        self,
        id: str,
        name: Optional[str] = None,
        is_active: Optional[bool] = None
    ) -> MutationResult:
        """Update an existing account"""
        return MutationResult(success=True, message="Account updated")

    @strawberry.mutation
    async def create_journal_entry(self, input: JournalEntryInput) -> MutationResult:
        """Create a new journal entry"""
        # Validate debits = credits
        total_debit = sum(line.debit for line in input.lines)
        total_credit = sum(line.credit for line in input.lines)

        if abs(total_debit - total_credit) > 0.001:
            return MutationResult(
                success=False,
                message=f"Journal entry is not balanced. Debits: {total_debit}, Credits: {total_credit}"
            )

        return MutationResult(success=True, message="Journal entry created", id="je-new-001")

    @strawberry.mutation
    async def create_transaction(self, input: TransactionInput) -> MutationResult:
        """Create a new transaction"""
        return MutationResult(success=True, message="Transaction created", id="txn-new-001")

    @strawberry.mutation
    async def convert_currency(self, input: CurrencyConversionInput) -> ConversionResult:
        """Convert currency using current exchange rates"""
        rates = {
            ("USD", "EUR"): 0.92,
            ("USD", "GBP"): 0.79,
            ("EUR", "USD"): 1.09,
            ("GBP", "USD"): 1.27,
            ("EUR", "GBP"): 0.86,
            ("GBP", "EUR"): 1.16
        }

        from_curr = input.from_currency.upper()
        to_curr = input.to_currency.upper()

        rate = rates.get((from_curr, to_curr), 1.0)
        converted = input.amount * rate

        return ConversionResult(
            from_currency=from_curr,
            to_currency=to_curr,
            original_amount=input.amount,
            converted_amount=converted,
            rate_used=rate
        )

    @strawberry.mutation
    async def acknowledge_alert(self, id: str) -> MutationResult:
        """Acknowledge an alert"""
        return MutationResult(success=True, message="Alert acknowledged")

    @strawberry.mutation
    async def resolve_alert(self, id: str, resolution_note: Optional[str] = None) -> MutationResult:
        """Resolve an alert"""
        return MutationResult(success=True, message="Alert resolved")

    @strawberry.mutation
    async def mark_notification_read(self, id: str, user_id: str) -> MutationResult:
        """Mark a notification as read"""
        return MutationResult(success=True, message="Notification marked as read")

# ============================================================================
# Create GraphQL Schema and Router
# ============================================================================

schema = strawberry.Schema(query=Query, mutation=Mutation)

graphql_app = GraphQLRouter(schema)

# Mount GraphQL endpoint
app.include_router(graphql_app, prefix="/graphql")

# Health check endpoint
@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "graphql-api"}

@app.get("/health")
async def health():
    return {"status": "healthy", "service": "graphql-api"}

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8095)