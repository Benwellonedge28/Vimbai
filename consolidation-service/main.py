"""
FinAcc Consolidation Service
Group financial reporting and consolidation for parent/subsidiary structures.
Handles elimination of intercompany transactions, minority interests, and
preparation of consolidated financial statements.
"""

import os
import uuid
from datetime import datetime, timedelta
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

# ============================================================================
# Configuration
# ============================================================================

SERVICE_NAME = "consolidation-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8021"))

# Internal service URLs
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")
REPORTING_SERVICE_URL = os.getenv("REPORTING_SERVICE_URL", "http://localhost:8002")
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ADMIN_SERVICE_URL = os.getenv("ADMIN_SERVICE_URL", "http://localhost:8001")
BUDGETING_SERVICE_URL = os.getenv("BUDGETING_SERVICE_URL", "http://localhost:8099")

# ============================================================================
# Logging Configuration
# ============================================================================

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

# ============================================================================
# FastAPI Application
# ============================================================================

app = FastAPI(
    title="FinAcc Consolidation Service",
    description="Group financial reporting and consolidation for parent/subsidiary structures",
    version=SERVICE_VERSION,
    docs_url="/docs",
    redoc_url="/redoc",
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


# ============================================================================
# Enums
# ============================================================================


class EntityType(str, Enum):
    PARENT = "parent"
    SUBSIDIARY = "subsidiary"
    ASSOCIATE = "associate"
    JOINT_VENTURE = "joint_venture"
    BRANCH = "branch"


class OwnershipType(str, Enum):
    DIRECT = "direct"
    INDIRECT = "indirect"
    EFFECTIVE = "effective"


class ConsolidationMethod(str, Enum):
    FULL_CONSOLIDATION = "full_consolidation"
    PROPORTIONATE_CONSOLIDATION = "proportionate_consolidation"
    EQUITY_METHOD = "equity_method"


class EliminationType(str, Enum):
    INTERCOMPANY_SALES = "intercompany_sales"
    INTERCOMPANY_PROFIT = "intercompany_profit"
    INTERCOMPANY_DIVIDENDS = "intercompany_dividends"
    INTERCOMPANY_LOANS = "intercompany_loans"
    INTERCOMPANY_REVENUE = "intercompany_revenue"
    INTERCOMPANY_EXPENSES = "intercompany_expenses"


class ReportType(str, Enum):
    CONSOLIDATED_BALANCE_SHEET = "consolidated_balance_sheet"
    CONSOLIDATED_INCOME = "consolidated_income_statement"
    CONSOLIDATED_CASH_FLOW = "consolidated_cash_flow"
    CONSOLIDATED_STATEMENT_OF_CHANGES = "consolidated_statement_of_changes"
    CONSOLIDATED_COMPREHENSIVE = "consolidated_comprehensive_income"
    FULL_CONSOLIDATED = "full_consolidated"


# ============================================================================
# Pydantic Models
# ============================================================================


class Subsidiary(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    entity_type: EntityType = EntityType.SUBSIDIARY
    registration_country: str
    functional_currency: str = "USD"
    reporting_currency: str = "USD"
    consolidation_method: ConsolidationMethod = ConsolidationMethod.FULL_CONSOLIDATION
    ownership_percentage: float = Field(ge=0, le=100)
    effective_interest: float = Field(ge=0, le=100)
    is_active: bool = True
    acquisition_date: Optional[datetime] = None
    minority_interest_percentage: float = Field(default=0, ge=0, le=100)
    fiscal_year_end: str = "12-31"
    is_foreign: bool = False
    exchange_rate_at_acquisition: float = 1.0
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)


class ConsolidationGroup(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    parent_entity_id: str
    subsidiaries: List[Subsidiary] = []
    reporting_currency: str = "USD"
    consolidation_date: datetime = Field(default_factory=datetime.utcnow)
    is_active: bool = True
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)


class IntercompanyTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    date: datetime
    transaction_type: str
    from_entity_id: str
    to_entity_id: str
    amount: float
    currency: str = "USD"
    description: str
    reference: Optional[str] = None
    is_eliminated: bool = False
    elimination_journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class EliminationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    consolidation_id: str
    elimination_type: EliminationType
    from_entity_id: str
    to_entity_id: str
    amount: float
    currency: str = "USD"
    journal_entry_id: Optional[str] = None
    description: str
    created_at: datetime = Field(default_factory=datetime.utcnow)


class GoodwillCalculation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    consolidation_id: str
    subsidiary_id: str
    acquisition_cost: float
    net_asset_fair_value: float
    goodwill_gross: float
    goodwill_net: float
    amortization_years: int = 10
    cumulative_amortization: float = 0
    net_goodwill: float = 0
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class MinorityInterest(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    consolidation_id: str
    subsidiary_id: str
    period_end: datetime
    minority_share_percentage: float
    net_assets: float
    net_profit: float
    other_comprehensive_income: float = 0
    total_comprehensive_income: float = 0
    minority_interest_net_assets: float = 0
    minority_interest_net_profit: float = 0
    created_at: datetime = Field(default_factory=datetime.utcnow)


class ConsolidatedFinancials(BaseModel):
    consolidation_id: str
    group_name: str
    report_date: datetime
    reporting_currency: str
    total_assets: float = 0
    total_liabilities: float = 0
    total_equity: float = 0
    minority_interest: float = 0
    parent_equity: float = 0
    total_revenue: float = 0
    total_expenses: float = 0
    net_profit: float = 0
    minority_share: float = 0
    parent_share: float = 0
    operating_cash_flow: float = 0
    investing_cash_flow: float = 0
    financing_cash_flow: float = 0
    net_cash_flow: float = 0
    goodwill_net: float = 0
    intercompany_eliminations: float = 0
    non_controlling_interest: float = 0


class ForeignSubsidiaryData(BaseModel):
    subsidiary_id: str
    period_start: datetime
    period_end: datetime
    exchange_rate_opening: float
    exchange_rate_closing: float
    exchange_rate_average: float
    revenue_local: float
    revenue_usd: float
    assets_local: float
    assets_usd: float
    equity_local: float
    equity_usd: float
    net_profit_local: float
    net_profit_usd: float
    cni_adjustment: float = 0


# ============================================================================
# In-Memory Storage
# ============================================================================

consolidation_groups: Dict[str, ConsolidationGroup] = {}
intercompany_transactions: List[IntercompanyTransaction] = []
elimination_entries: List[EliminationEntry] = []
goodwill_calculations: List[GoodwillCalculation] = []
minority_interests: List[MinorityInterest] = []
foreign_subsidiary_data: Dict[str, List[ForeignSubsidiaryData]] = {}


# ============================================================================
# Internal Service Communication
# ============================================================================


async def call_accounting_service(
    method: str, endpoint: str, data: Optional[Dict] = None
) -> Dict[str, Any]:
    """Call the main accounting service."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "GET":
                response = await client.get(url)
            elif method == "POST":
                response = await client.post(url, json=data)
            else:
                raise ValueError(f"Unsupported HTTP method: {method}")

            if response.status_code in [200, 201]:
                return response.json() if response.text else {}
            return {}
    except Exception as e:
        logger.error("accounting_service_call_error", error=str(e))
        return {}


async def call_audit_service(
    action: str, resource_type: str, resource_id: str, details: Dict[str, Any]
) -> None:
    """Log actions to the audit service."""
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            await client.post(
                f"{AUDIT_SERVICE_URL}/audit",
                json={
                    "action": action,
                    "resource_type": resource_type,
                    "resource_id": resource_id,
                    "details": details,
                    "timestamp": datetime.utcnow().isoformat(),
                },
            )
    except Exception as e:
        logger.error("audit_service_call_error", error=str(e))


# ============================================================================
# Helper Functions
# ============================================================================


def calculate_effective_ownership(
    direct_ownership: float, parent_effective_ownership: float
) -> float:
    """Calculate effective ownership percentage through chain."""
    return (direct_ownership / 100) * (parent_effective_ownership / 100) * 100


def get_exchange_rate(from_currency: str, to_currency: str) -> float:
    """Get exchange rate between currencies (simplified)."""
    if from_currency == to_currency:
        return 1.0
    rates = {
        ("USD", "EUR"): 0.92, ("EUR", "USD"): 1.09,
        ("USD", "GBP"): 0.79, ("GBP", "USD"): 1.27,
        ("USD", "JPY"): 149.50, ("JPY", "USD"): 0.0067,
    }
    return rates.get((from_currency, to_currency), 1.0)


def convert_currency(amount: float, from_currency: str, to_currency: str) -> float:
    """Convert amount between currencies."""
    rate = get_exchange_rate(from_currency, to_currency)
    return amount * rate


# ============================================================================
# API Endpoints - Health & Info
# ============================================================================


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "status": "healthy",
        "timestamp": datetime.utcnow().isoformat(),
    }


@app.get("/")
async def root():
    """Root endpoint."""
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "description": "Group financial reporting and consolidation",
    }


# ============================================================================
# API Endpoints - Consolidation Group Management
# ============================================================================


@app.post("/consolidation-groups", response_model=ConsolidationGroup, status_code=status.HTTP_201_CREATED)
async def create_consolidation_group(data: ConsolidationGroup):
    """Create a new consolidation group."""
    group_id = str(uuid.uuid4())
    data.id = group_id
    data.created_at = datetime.utcnow()
    data.updated_at = datetime.utcnow()
    consolidation_groups[group_id] = data

    await call_audit_service("CREATE", "consolidation_group", group_id, {"name": data.name})
    logger.info("consolidation_group_created", group_id=group_id, name=data.name)
    return data


@app.get("/consolidation-groups/{group_id}")
async def get_consolidation_group(group_id: str):
    """Get consolidation group details."""
    group = consolidation_groups.get(group_id)
    if not group:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Group {group_id} not found")
    return group


@app.get("/consolidation-groups")
async def list_consolidation_groups(is_active: Optional[bool] = None):
    """List all consolidation groups."""
    result = list(consolidation_groups.values())
    if is_active is not None:
        result = [g for g in result if g.is_active == is_active]
    return {"groups": result, "count": len(result)}


@app.post("/consolidation-groups/{group_id}/subsidiaries", response_model=Subsidiary, status_code=status.HTTP_201_CREATED)
async def add_subsidiary(group_id: str, data: Subsidiary):
    """Add a subsidiary to the consolidation group."""
    group = consolidation_groups.get(group_id)
    if not group:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Group {group_id} not found")

    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()
    data.updated_at = datetime.utcnow()
    data.effective_interest = data.ownership_percentage
    data.minority_interest_percentage = 100 - data.ownership_percentage

    group.subsidiaries.append(data)
    group.updated_at = datetime.utcnow()

    await call_audit_service("CREATE", "subsidiary", data.id, {"group_id": group_id, "name": data.name})
    return data


# ============================================================================
# API Endpoints - Intercompany Transactions
# ============================================================================


@app.post("/intercompany-transactions", response_model=IntercompanyTransaction, status_code=status.HTTP_201_CREATED)
async def record_intercompany_transaction(data: IntercompanyTransaction):
    """Record an intercompany transaction for later elimination."""
    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()
    intercompany_transactions.append(data)

    await call_audit_service("CREATE", "intercompany_transaction", data.id, {
        "from_entity": data.from_entity_id, "to_entity": data.to_entity_id, "amount": data.amount
    })
    return data


@app.get("/intercompany-transactions")
async def get_intercompany_transactions(
    from_entity_id: Optional[str] = None, to_entity_id: Optional[str] = None,
    is_eliminated: Optional[bool] = None,
):
    """Get intercompany transactions with filters."""
    result = intercompany_transactions
    if from_entity_id:
        result = [t for t in result if t.from_entity_id == from_entity_id]
    if to_entity_id:
        result = [t for t in result if t.to_entity_id == to_entity_id]
    if is_eliminated is not None:
        result = [t for t in result if t.is_eliminated == is_eliminated]

    total = sum(t.amount for t in result)
    return {"transactions": result, "total": total, "count": len(result)}


@app.post("/intercompany-transactions/{transaction_id}/eliminate")
async def eliminate_intercompany_transaction(transaction_id: str):
    """Mark an intercompany transaction as eliminated."""
    transaction = next((t for t in intercompany_transactions if t.id == transaction_id), None)
    if not transaction:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Transaction {transaction_id} not found")

    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Elimination of intercompany transaction: {transaction.description}",
        "entries": [
            {"account_code": "4000", "description": "Revenue elimination", "debit": transaction.amount, "credit": 0},
            {"account_code": "1200", "description": "Receivable elimination", "debit": 0, "credit": transaction.amount},
        ],
        "reference": f"IEC-ELIM-{transaction_id[:8]}",
    }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    transaction.is_eliminated = True
    transaction.elimination_journal_entry_id = result.get("id")

    await call_audit_service("ELIMINATE", "intercompany_transaction", transaction_id, {})
    return {"message": "Transaction eliminated", "transaction": transaction}


# ============================================================================
# API Endpoints - Goodwill Calculation
# ============================================================================


@app.post("/consolidation-groups/{group_id}/subsidiaries/{subsidiary_id}/goodwill", response_model=GoodwillCalculation)
async def calculate_goodwill_for_acquisition(
    group_id: str, subsidiary_id: str, acquisition_cost: float, non_controlling_interest_fair_value: float = 0,
):
    """Calculate and record goodwill from acquisition."""
    group = consolidation_groups.get(group_id)
    if not group:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Group {group_id} not found")

    subsidiary = next((s for s in group.subsidiaries if s.id == subsidiary_id), None)
    if not subsidiary:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Subsidiary {subsidiary_id} not found")

    net_asset_fair_value = 1000000
    total_consideration = acquisition_cost + non_controlling_interest_fair_value
    goodwill_gross = total_consideration - net_asset_fair_value

    calculation = GoodwillCalculation(
        consolidation_id=group_id, subsidiary_id=subsidiary_id,
        acquisition_cost=acquisition_cost, net_asset_fair_value=net_asset_fair_value,
        goodwill_gross=goodwill_gross, goodwill_net=goodwill_gross, net_goodwill=goodwill_gross,
    )

    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Goodwill on acquisition of {subsidiary.name}",
        "entries": [
            {"account_code": "1500", "description": "Goodwill", "debit": goodwill_gross, "credit": 0},
            {"account_code": "1000", "description": "Cash/Bank", "debit": 0, "credit": acquisition_cost},
            {"account_code": "2100", "description": "Non-controlling interest", "debit": 0, "credit": non_controlling_interest_fair_value},
        ],
        "reference": f"GW-{calculation.id[:8]}",
    }

    result = await call_accounting_service("POST", "/journal-entries", journal_entry)
    calculation.journal_entry_id = result.get("id")
    goodwill_calculations.append(calculation)

    await call_audit_service("CALCULATE", "goodwill", calculation.id, {"goodwill_amount": goodwill_gross})
    return calculation


# ============================================================================
# API Endpoints - Minority Interest
# ============================================================================


@app.post("/consolidation-groups/{group_id}/subsidiaries/{subsidiary_id}/minority-interest", response_model=MinorityInterest)
async def calculate_minority_interest(
    group_id: str, subsidiary_id: str, period_end: datetime, net_assets: float, net_profit: float,
    other_comprehensive_income: float = 0,
):
    """Calculate minority interest for a subsidiary."""
    group = consolidation_groups.get(group_id)
    if not group:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Group {group_id} not found")

    subsidiary = next((s for s in group.subsidiaries if s.id == subsidiary_id), None)
    if not subsidiary:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Subsidiary {subsidiary_id} not found")

    minority_percentage = subsidiary.minority_interest_percentage / 100

    interest = MinorityInterest(
        consolidation_id=group_id, subsidiary_id=subsidiary_id, period_end=period_end,
        minority_share_percentage=subsidiary.minority_interest_percentage,
        net_assets=net_assets, net_profit=net_profit, other_comprehensive_income=other_comprehensive_income,
        total_comprehensive_income=net_profit + other_comprehensive_income,
        minority_interest_net_assets=net_assets * minority_percentage,
        minority_interest_net_profit=net_profit * minority_percentage,
    )

    minority_interests.append(interest)
    await call_audit_service("CALCULATE", "minority_interest", interest.id, {"amount": interest.minority_interest_net_profit})
    return interest


# ============================================================================
# API Endpoints - Consolidated Financial Statements
# ============================================================================


@app.post("/consolidation-groups/{group_id}/consolidate", response_model=ConsolidatedFinancials)
async def prepare_consolidated_financials(group_id: str, period_end: datetime):
    """Prepare consolidated financial statements."""
    group = consolidation_groups.get(group_id)
    if not group:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail=f"Group {group_id} not found")

    consolidated = ConsolidatedFinancials(
        consolidation_id=group_id, group_name=group.name, report_date=period_end,
        reporting_currency=group.reporting_currency,
    )

    total_revenue = 0
    total_expenses = 0
    total_assets = 0
    total_liabilities = 0
    total_goodwill = 0

    for subsidiary in group.subsidiaries:
        if not subsidiary.is_active:
            continue

        if subsidiary.consolidation_method == ConsolidationMethod.FULL_CONSOLIDATION:
            factor = 1.0
        elif subsidiary.consolidation_method == ConsolidationMethod.PROPORTIONATE_CONSOLIDATION:
            factor = subsidiary.ownership_percentage / 100
        else:
            factor = 0

        total_revenue += 1000000 * factor
        total_expenses += 800000 * factor
        total_assets += 5000000 * factor
        total_liabilities += 3000000 * factor

        gw_calc = next((g for g in goodwill_calculations if g.subsidiary_id == subsidiary.id), None)
        if gw_calc:
            total_goodwill += gw_calc.net_goodwill

    consolidated.total_revenue = total_revenue
    consolidated.total_expenses = total_expenses
    consolidated.total_assets = total_assets + total_goodwill
    consolidated.total_liabilities = total_liabilities
    consolidated.total_equity = consolidated.total_assets - consolidated.total_liabilities
    consolidated.goodwill_net = total_goodwill
    consolidated.net_profit = total_revenue - total_expenses

    group_minority = sum(m.minority_interest_net_profit for m in minority_interests if m.consolidation_id == group_id)
    consolidated.minority_share = group_minority
    consolidated.parent_share = consolidated.net_profit - group_minority

    group_nci = sum(m.minority_interest_net_assets for m in minority_interests if m.consolidation_id == group_id)
    consolidated.minority_interest = group_nci
    consolidated.parent_equity = consolidated.total_equity - group_nci

    eliminated = sum(t.amount for t in intercompany_transactions if t.is_eliminated and t.date <= period_end)
    consolidated.intercompany_eliminations = eliminated
    consolidated.total_revenue -= eliminated

    await call_audit_service("CONSOLIDATE", "financials", group_id, {"period_end": period_end.isoformat()})
    return consolidated


# ============================================================================
# Main Entry Point
# ============================================================================


if __name__ == "__main__":
    import uvicorn
    logger.info("starting_consolidation_service", port=PORT)
    uvicorn.run(app, host="0.0.0.0", port=PORT)