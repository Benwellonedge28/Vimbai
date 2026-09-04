"""
NPO Service CRUD Operations

Comprehensive CRUD operations for NPO accounting including:
- Fund Accounting (General, Restricted, Endowment, Capital, Project)
- Net Assets Management
- Revenue and Grant Tracking
- Budget and Cost Allocation
- Project and Program Management
- Donor Management
- Compliance and Governance
- Performance and Impact Measurement
"""

import os
import uuid
from datetime import date, datetime, timedelta, timezone
from decimal import Decimal
from typing import Any, Dict, List, Optional

import httpx
from neo4j import AsyncSession
from npo_service.dependencies import book_id_var
from npo_service.exceptions import ConflictError, NotFoundError, RestrictionViolationError, ValidationError
from npo_service.models import (
    AccruedExpenseBase,
    AssetStatus,
    AuditReportCreate,
    AuditReportInDB,
    BeneficiaryAccountabilityBase,
    BudgetCreate,
    BudgetInDB,
    BudgetLineCreate,
    BudgetLineInDB,
    BudgetStatus,
    ComplianceCheckBase,
    CostAllocationBase,
    CostCenterBase,
    DeferredRevenueBase,
    DepreciationEntryBase,
    DonationCreate,
    DonationInDB,
    DonorCreate,
    DonorInDB,
    DonorReportBase,
    DonorStewardshipBase,
    EndowmentAssetBase,
    FundCreate,
    FundInDB,
    FundraisingEventBase,
    FundRestrictionCreate,
    FundRestrictionInDB,
    FundTransactionCreate,
    FundTransactionInDB,
    FundType,
    GrantCreate,
    GrantDrawdownBase,
    GrantInDB,
    GrantStatus,
    ImpactMeasurementCreate,
    InKindContributionCreate,
    InternalControlCreate,
    InternalControlInDB,
    InvestmentIncomeBase,
    LiabilityBase,
    MembershipFeeCreate,
    NetAssetsChangeBase,
    NetAssetsInDB,
    NPOAssetCreate,
    NPOAssetInDB,
    ProgramCreate,
    ProgramInDB,
    ProgramMetricCreate,
    ProjectCreate,
    ProjectInDB,
    ProjectStatus,
    RegulatoryFilingBase,
    SROIAnalysisBase,
    StatementOfActivitiesInDB,
    StatementOfCashFlowsInDB,
    StatementOfChangesInNetAssetsInDB,
    StatementOfFinancialPositionInDB,
    SustainabilityReportBase,
    VolunteerRecordCreate,
    VolunteerRecordInDB,
)

API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")

# =============================================================================
# FUND ACCOUNTING CRUD (1-15)
# =============================================================================


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


async def create_fund(session: AsyncSession, user_id: str, fund: FundCreate) -> FundInDB:
    """Create a new NPO fund"""
    fund_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = created_at

    # Check for existing fund code
    existing = await get_fund_by_code(session, user_id, fund.fund_code)
    if existing:
        raise ConflictError(detail=f"Fund code {fund.fund_code} already exists", code="FUND_CODE_EXISTS")

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (f:NPOFund {
        id: $id,
        book_id: $book_id,
        fund_code: $fund_code,
        fund_name: $fund_name,
        fund_type: $fund_type,
        description: $description,
        purpose: $purpose,
        current_balance: toFloat($current_balance),
        total_contributions: toFloat($total_contributions),
        total_disbursements: toFloat($total_disbursements),
        currency: $currency,
        parent_fund_id: $parent_fund_id,
        status: $status,
        created_date: $created_date,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_NPO_FUND]->(f)
    RETURN f
    """
    params = {
        "id": fund_id,
        "user_id": user_id,
        "fund_code": fund.fund_code,
        "fund_name": fund.fund_name,
        "fund_type": fund.fund_type.value,
        "description": fund.description,
        "purpose": fund.purpose,
        "current_balance": float(fund.initial_balance),
        "total_contributions": float(fund.initial_balance),
        "total_disbursements": 0.0,
        "currency": fund.currency,
        "parent_fund_id": fund.parent_fund_id,
        "status": "active",
        "created_date": fund.created_date.isoformat() if fund.created_date else None,
        "created_at": created_at.isoformat(),
        "updated_at": updated_at.isoformat(),
    }

    result = await _run(session, query, params)
    record = await result.single()
    f = record["f"]

    return FundInDB(
        id=f["id"],
        user_id=user_id,
        fund_code=f["fund_code"],
        fund_name=f["fund_name"],
        fund_type=FundType(f["fund_type"]),
        description=f["description"],
        purpose=f["purpose"],
        current_balance=Decimal(str(f["current_balance"])),
        total_contributions=Decimal(str(f["total_contributions"])),
        total_disbursements=Decimal(str(f["total_disbursements"])),
        currency=f["currency"],
        parent_fund_id=f["parent_fund_id"],
        status=f["status"],
        created_date=f.get("created_date"),
        created_at=datetime.fromisoformat(f["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(f["updated_at"].iso_format()),
    )


async def get_fund(session: AsyncSession, user_id: str, fund_id: str) -> FundInDB:
    """Get fund by ID"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_NPO_FUND]->(f:NPOFund {id: $fund_id})
    WHERE $book_id IS NULL OR f.book_id = $book_id
    RETURN f
    """
    result = await _run(session, query, user_id=user_id, fund_id=fund_id)
    record = await result.single()
    if not record:
        raise NotFoundError(detail=f"Fund {fund_id} not found", code="FUND_NOT_FOUND")
    f = record["f"]
    return FundInDB(
        id=f["id"],
        user_id=user_id,
        fund_code=f["fund_code"],
        fund_name=f["fund_name"],
        fund_type=FundType(f["fund_type"]),
        description=f["description"],
        purpose=f["purpose"],
        current_balance=Decimal(str(f["current_balance"])),
        total_contributions=Decimal(str(f["total_contributions"])),
        total_disbursements=Decimal(str(f["total_disbursements"])),
        currency=f["currency"],
        parent_fund_id=f["parent_fund_id"],
        status=f["status"],
        created_date=f.get("created_date"),
        created_at=datetime.fromisoformat(f["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(f["updated_at"].iso_format()),
    )


async def get_fund_by_code(session: AsyncSession, user_id: str, fund_code: str) -> Optional[FundInDB]:
    """Get fund by fund code"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_NPO_FUND]->(f:NPOFund {fund_code: $fund_code})
    WHERE $book_id IS NULL OR f.book_id = $book_id
    RETURN f
    """
    result = await _run(session, query, user_id=user_id, fund_code=fund_code)
    try:
        record = await result.single()
        if record:
            f = record["f"]
            return FundInDB(
                id=f["id"],
                user_id=user_id,
                fund_code=f["fund_code"],
                fund_name=f["fund_name"],
                fund_type=FundType(f["fund_type"]),
                description=f["description"],
                purpose=f["purpose"],
                current_balance=Decimal(str(f["current_balance"])),
                total_contributions=Decimal(str(f["total_contributions"])),
                total_disbursements=Decimal(str(f["total_disbursements"])),
                currency=f["currency"],
                parent_fund_id=f["parent_fund_id"],
                status=f["status"],
                created_date=f.get("created_date"),
                created_at=datetime.fromisoformat(f["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(f["updated_at"].iso_format()),
            )
    except Exception:
        pass
    return None


async def get_all_funds(session: AsyncSession, user_id: str, fund_type: Optional[str] = None) -> List[FundInDB]:
    """Get all funds, optionally filtered by type"""
    type_filter = "AND f.fund_type = $fund_type" if fund_type else ""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_NPO_FUND]->(f:NPOFund)
    WHERE true {type_filter} AND ($book_id IS NULL OR f.book_id = $book_id)
    RETURN f
    ORDER BY f.fund_code
    """
    params = {"user_id": user_id}
    if fund_type:
        params["fund_type"] = fund_type

    result = await _run(session, query, params)
    funds = []
    async for record in result:
        f = record["f"]
        funds.append(
            FundInDB(
                id=f["id"],
                user_id=user_id,
                fund_code=f["fund_code"],
                fund_name=f["fund_name"],
                fund_type=FundType(f["fund_type"]),
                description=f["description"],
                purpose=f["purpose"],
                current_balance=Decimal(str(f["current_balance"])),
                total_contributions=Decimal(str(f["total_contributions"])),
                total_disbursements=Decimal(str(f["total_disbursements"])),
                currency=f["currency"],
                parent_fund_id=f["parent_fund_id"],
                status=f["status"],
                created_date=f.get("created_date"),
                created_at=datetime.fromisoformat(f["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(f["updated_at"].iso_format()),
            )
        )
    return funds


async def update_fund_balance(session: AsyncSession, fund_id: str, amount: Decimal, is_contribution: bool):
    """Update fund balance after transaction"""
    if is_contribution:
        query = """
        MATCH (f:NPOFund {id: $fund_id})
        WHERE $book_id IS NULL OR f.book_id = $book_id
        SET f.current_balance = f.current_balance + toFloat($amount),
            f.total_contributions = f.total_contributions + toFloat($amount),
            f.updated_at = datetime($updated_at)
        """
    else:
        query = """
        MATCH (f:NPOFund {id: $fund_id})
        WHERE $book_id IS NULL OR f.book_id = $book_id
        SET f.current_balance = f.current_balance - toFloat($amount),
            f.total_disbursements = f.total_disbursements + toFloat($amount),
            f.updated_at = datetime($updated_at)
        """
    await _run(session, query, fund_id=fund_id, amount=float(amount), updated_at=datetime.now(timezone.utc).isoformat())


async def create_fund_transaction(
    session: AsyncSession, user_id: str, fund_id: str, transaction: FundTransactionCreate
) -> FundTransactionInDB:
    """Create fund transaction with balance update"""
    tx_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    # Get current balance
    fund = await get_fund(session, user_id, fund_id)

    # Calculate new balance
    if transaction.transaction_type in ["contribution", "transfer_in", "investment", "appreciation"]:
        new_balance = fund.current_balance + transaction.amount
        is_contribution = True
    else:
        new_balance = fund.current_balance - transaction.amount
        is_contribution = False

    # Check restriction for restricted funds
    if fund.fund_type in [FundType.PERMANENTLY_RESTRICTED, FundType.ENDOWMENT]:
        if transaction.transaction_type not in ["investment", "appreciation"]:
            raise RestrictionViolationError(
                detail=f"Cannot make disbursement from {fund.fund_type.value} fund", code="FUND_RESTRICTION_VIOLATION"
            )

    # Create transaction
    query = """
    MATCH (u:User {id: $user_id}), (f:NPOFund {id: $fund_id})
    WHERE $book_id IS NULL OR f.book_id = $book_id
    CREATE (tx:NPOFundTransaction {
        id: $id,
        book_id: $book_id,
        transaction_date: date($transaction_date),
        transaction_type: $transaction_type,
        amount: toFloat($amount),
        description: $description,
        reference_number: $reference_number,
        category: $category,
        project_id: $project_id,
        grant_id: $grant_id,
        donor_id: $donor_id,
        created_by: $created_by,
        balance_after: toFloat($balance_after),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_NPO_FUND_TRANSACTION]->(tx)
    CREATE (tx)-[:TRANSACTION_IN_FUND]->(f)
    RETURN tx, f
    """
    params = {
        "id": tx_id,
        "user_id": user_id,
        "fund_id": fund_id,
        "transaction_date": transaction.transaction_date.isoformat(),
        "transaction_type": transaction.transaction_type,
        "amount": float(transaction.amount),
        "description": transaction.description,
        "reference_number": transaction.reference_number,
        "category": transaction.category,
        "project_id": transaction.project_id,
        "grant_id": transaction.grant_id,
        "donor_id": transaction.donor_id,
        "created_by": transaction.created_by,
        "balance_after": float(new_balance),
        "created_at": created_at.isoformat(),
    }

    result = await _run(session, query, params)
    record = await result.single()
    tx = record["tx"]

    # Update fund balance
    await update_fund_balance(session, fund_id, transaction.amount, is_contribution)

    return FundTransactionInDB(
        id=tx["id"],
        fund_id=fund_id,
        transaction_date=datetime.fromisoformat(tx["transaction_date"].iso_format()).date(),
        transaction_type=tx["transaction_type"],
        amount=Decimal(str(tx["amount"])),
        description=tx["description"],
        reference_number=tx["reference_number"],
        category=tx["category"],
        project_id=tx["project_id"],
        grant_id=tx["grant_id"],
        donor_id=tx["donor_id"],
        created_by=tx["created_by"],
        balance_after=Decimal(str(tx["balance_after"])),
        created_at=datetime.fromisoformat(tx["created_at"].iso_format()),
    )


async def get_fund_transactions(
    session: AsyncSession, user_id: str, fund_id: str, start_date=None, end_date=None
) -> List[FundTransactionInDB]:
    """Get transactions for a fund"""
    date_filter = ""
    params = {"user_id": user_id, "fund_id": fund_id}
    if start_date:
        date_filter += " AND tx.transaction_date >= date($start_date)"
        params["start_date"] = start_date.isoformat()
    if end_date:
        date_filter += " AND tx.transaction_date <= date($end_date)"
        params["end_date"] = end_date.isoformat()

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_NPO_FUND_TRANSACTION]->(tx:NPOFundTransaction)-[:TRANSACTION_IN_FUND]->(f:NPOFund {{id: $fund_id}})
    WHERE true {date_filter} AND ($book_id IS NULL OR tx.book_id = $book_id)
    RETURN tx
    ORDER BY tx.transaction_date DESC
    """
    result = await _run(session, query, params)
    transactions = []
    async for record in result:
        tx = record["tx"]
        transactions.append(
            FundTransactionInDB(
                id=tx["id"],
                fund_id=fund_id,
                transaction_date=datetime.fromisoformat(tx["transaction_date"].iso_format()).date(),
                transaction_type=tx["transaction_type"],
                amount=Decimal(str(tx["amount"])),
                description=tx["description"],
                reference_number=tx["reference_number"],
                category=tx["category"],
                project_id=tx.get("project_id"),
                grant_id=tx.get("grant_id"),
                donor_id=tx.get("donor_id"),
                created_by=tx.get("created_by"),
                balance_after=Decimal(str(tx["balance_after"])),
                created_at=datetime.fromisoformat(tx["created_at"].iso_format()),
            )
        )
    return transactions


async def create_fund_restriction(
    session: AsyncSession, user_id: str, fund_id: str, restriction: FundRestrictionCreate
) -> FundRestrictionInDB:
    """Create fund restriction"""
    restriction_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    query = """
    MATCH (u:User {id: $user_id}), (f:NPOFund {id: $fund_id})
    WHERE $book_id IS NULL OR f.book_id = $book_id
    CREATE (r:FundRestriction {
        id: $id,
        book_id: $book_id,
        restriction_type: $restriction_type,
        description: $description,
        start_date: $start_date,
        end_date: $end_date,
        is_permanent: $is_permanent,
        terms_conditions: $terms_conditions,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_FUND_RESTRICTION]->(r)
    CREATE (r)-[:RESTRICTS_FUND]->(f)
    RETURN r
    """
    params = {
        "id": restriction_id,
        "user_id": user_id,
        "fund_id": fund_id,
        "restriction_type": restriction.restriction_type,
        "description": restriction.description,
        "start_date": restriction.start_date.isoformat() if restriction.start_date else None,
        "end_date": restriction.end_date.isoformat() if restriction.end_date else None,
        "is_permanent": restriction.is_permanent,
        "terms_conditions": restriction.terms_conditions,
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    if not record:
        raise NotFoundError(detail=f"Fund {fund_id} not found", code="FUND_NOT_FOUND")
    r = record["r"]

    return FundRestrictionInDB(
        id=r["id"],
        fund_id=fund_id,
        restriction_type=r["restriction_type"],
        description=r["description"],
        start_date=datetime.fromisoformat(r["start_date"].iso_format()).date() if r.get("start_date") else None,
        end_date=datetime.fromisoformat(r["end_date"].iso_format()).date() if r.get("end_date") else None,
        is_permanent=r["is_permanent"],
        terms_conditions=r.get("terms_conditions"),
        created_at=datetime.fromisoformat(r["created_at"].iso_format()),
    )


# =============================================================================
# NET ASSETS CRUD
# =============================================================================


async def create_net_assets(
    session: AsyncSession, user_id: str, as_of_date: date, period_start: date, period_end: date
) -> NetAssetsInDB:
    """Create net assets record"""
    assets_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    # Calculate totals from funds
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_NPO_FUND]->(f:NPOFund)
    WHERE f.status = 'active' AND ($book_id IS NULL OR f.book_id = $book_id)
    RETURN f
    """
    result = await _run(session, query, user_id=user_id)

    net_assets_without = Decimal("0.00")
    net_assets_with = Decimal("0.00")
    total_balance = Decimal("0.00")

    async for record in result:
        f = record["f"]
        balance = Decimal(str(f["current_balance"]))
        total_balance += balance

        if f["fund_type"] in ["general", "operating", "board_designated"]:
            net_assets_without += balance
        else:
            net_assets_with += balance

    beginning = total_balance  # Simplified
    change = Decimal("0.00")

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (na:NetAssets {
        id: $id,
        book_id: $book_id,
        as_of_date: date($as_of_date),
        period_start: date($period_start),
        period_end: date($period_end),
        net_assets_without_donor_restrictions: toFloat($without),
        net_assets_with_donor_restrictions: toFloat($with),
        net_assets_with_permanent_restrictions: toFloat($permanent),
        net_assets_with_temporary_restrictions: toFloat($temporary),
        endowment_net_assets: toFloat($endowment),
        board_designated_net_assets: toFloat($board),
        total_net_assets: toFloat($total),
        accumulated_surplus: toFloat($surplus),
        accumulated_deficit: toFloat($deficit),
        beginning_net_assets: toFloat($beginning),
        net_assets_change: toFloat($change),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_NET_ASSETS]->(na)
    RETURN na
    """
    params = {
        "id": assets_id,
        "user_id": user_id,
        "as_of_date": as_of_date.isoformat(),
        "period_start": period_start.isoformat(),
        "period_end": period_end.isoformat(),
        "without": float(net_assets_without),
        "with": float(net_assets_with),
        "permanent": 0.0,
        "temporary": 0.0,
        "endowment": 0.0,
        "board": 0.0,
        "total": float(total_balance),
        "surplus": total_balance if total_balance > 0 else 0.0,
        "deficit": abs(total_balance) if total_balance < 0 else 0.0,
        "beginning": float(beginning),
        "change": float(change),
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    na = record["na"]

    return NetAssetsInDB(
        id=na["id"],
        user_id=user_id,
        as_of_date=datetime.fromisoformat(na["as_of_date"].iso_format()).date(),
        period_start=datetime.fromisoformat(na["period_start"].iso_format()).date(),
        period_end=datetime.fromisoformat(na["period_end"].iso_format()).date(),
        net_assets_without_donor_restrictions=Decimal(str(na["net_assets_without_donor_restrictions"])),
        net_assets_with_donor_restrictions=Decimal(str(na["net_assets_with_donor_restrictions"])),
        net_assets_with_permanent_restrictions=Decimal(str(na["net_assets_with_permanent_restrictions"])),
        net_assets_with_temporary_restrictions=Decimal(str(na["net_assets_with_temporary_restrictions"])),
        endowment_net_assets=Decimal(str(na["endowment_net_assets"])),
        board_designated_net_assets=Decimal(str(na["board_designated_net_assets"])),
        total_net_assets=Decimal(str(na["total_net_assets"])),
        accumulated_surplus=Decimal(str(na["accumulated_surplus"])),
        accumulated_deficit=Decimal(str(na["accumulated_deficit"])),
        beginning_net_assets=Decimal(str(na["beginning_net_assets"])),
        net_assets_change=Decimal(str(na["net_assets_change"])),
        created_at=datetime.fromisoformat(na["created_at"].iso_format()),
    )


async def get_net_assets(session: AsyncSession, user_id: str, as_of_date: date) -> NetAssetsInDB:
    """Get net assets as of date"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_NET_ASSETS]->(na:NetAssets)
    WHERE na.as_of_date = date($as_of_date) AND ($book_id IS NULL OR na.book_id = $book_id)
    RETURN na
    """
    result = await _run(session, query, user_id=user_id, as_of_date=as_of_date.isoformat())
    record = await result.single()
    if not record:
        raise NotFoundError(detail=f"No net assets found for date {as_of_date}")
    na = record["na"]
    return NetAssetsInDB(
        id=na["id"],
        user_id=user_id,
        as_of_date=datetime.fromisoformat(na["as_of_date"].iso_format()).date(),
        period_start=datetime.fromisoformat(na["period_start"].iso_format()).date(),
        period_end=datetime.fromisoformat(na["period_end"].iso_format()).date(),
        net_assets_without_donor_restrictions=Decimal(str(na["net_assets_without_donor_restrictions"])),
        net_assets_with_donor_restrictions=Decimal(str(na["net_assets_with_donor_restrictions"])),
        total_net_assets=Decimal(str(na["total_net_assets"])),
        accumulated_surplus=Decimal(str(na["accumulated_surplus"])),
        accumulated_deficit=Decimal(str(na["accumulated_deficit"])),
        beginning_net_assets=Decimal(str(na["beginning_net_assets"])),
        net_assets_change=Decimal(str(na["net_assets_change"])),
        created_at=datetime.fromisoformat(na["created_at"].iso_format()),
    )


# =============================================================================
# REVENUE CRUD (Donations, Grants, Memberships)
# =============================================================================


async def create_donation(session: AsyncSession, user_id: str, donation: DonationCreate) -> DonationInDB:
    """Create donation and update fund balance"""
    donation_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    receipt_number = f"DON-{datetime.now(timezone.utc).strftime('%Y%m%d')}-{donation_id[:8]}"

    # Determine target fund
    target_fund_id = donation.fund_id
    if not target_fund_id:
        # Get or create general fund
        general_fund = await get_all_funds(session, user_id, FundType.GENERAL.value)
        if general_fund:
            target_fund_id = general_fund[0].id
        else:
            # Create general fund if doesn't exist
            fund_create = FundCreate(
                fund_code="GEN-001",
                fund_name="General Fund",
                fund_type=FundType.GENERAL,
                purpose="Unrestricted operating funds",
                initial_balance=Decimal("0.00"),
            )
            new_fund = await create_fund(session, user_id, fund_create)
            target_fund_id = new_fund.id

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (d:Donation {
        id: $id,
        book_id: $book_id,
        donation_date: date($donation_date),
        amount: toFloat($amount),
        donor_id: $donor_id,
        donation_type: $donation_type,
        payment_method: $payment_method,
        campaign: $campaign,
        appeal: $appeal,
        is_anonymous: $is_anonymous,
        acknowledgement_sent: $acknowledgement_sent,
        tax_deductible: $tax_deductible,
        notes: $notes,
        receipt_number: $receipt_number,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_DONATION]->(d)
    RETURN d
    """
    params = {
        "id": donation_id,
        "user_id": user_id,
        "donation_date": donation.donation_date.isoformat(),
        "amount": float(donation.amount),
        "donor_id": donation.donor_id,
        "donation_type": donation.donation_type,
        "payment_method": donation.payment_method,
        "campaign": donation.campaign,
        "appeal": donation.appeal,
        "is_anonymous": donation.is_anonymous,
        "acknowledgement_sent": donation.acknowledgement_sent,
        "tax_deductible": donation.tax_deductible,
        "notes": donation.notes,
        "receipt_number": receipt_number,
        "created_at": created_at.isoformat(),
    }
    await _run(session, query, params)

    # Update fund balance
    if target_fund_id:
        tx_create = FundTransactionCreate(
            transaction_date=donation.donation_date,
            transaction_type="contribution",
            amount=donation.amount,
            description=f"Donation from {donation.donor_id} - {receipt_number}",
            donor_id=donation.donor_id,
        )
        await create_fund_transaction(session, user_id, target_fund_id, tx_create)

    return DonationInDB(
        id=donation_id,
        user_id=user_id,
        donation_date=donation.donation_date,
        amount=donation.amount,
        donor_id=donation.donor_id,
        donation_type=donation.donation_type,
        payment_method=donation.payment_method,
        campaign=donation.campaign,
        appeal=donation.appeal,
        is_anonymous=donation.is_anonymous,
        acknowledgement_sent=donation.acknowledgement_sent,
        tax_deductible=donation.tax_deductible,
        notes=donation.notes,
        receipt_number=receipt_number,
        created_at=created_at,
    )


async def get_donations(session: AsyncSession, user_id: str) -> List[DonationInDB]:
    """Get all donations for the user, scoped to the active Book"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_DONATION]->(d:Donation)
    WHERE $book_id IS NULL OR d.book_id = $book_id
    RETURN d
    ORDER BY d.donation_date DESC
    """
    result = await _run(session, query, user_id=user_id)
    donations = []
    async for record in result:
        d = record["d"]
        donations.append(
            DonationInDB(
                id=d["id"],
                user_id=user_id,
                donation_date=datetime.fromisoformat(d["donation_date"].iso_format()).date(),
                amount=Decimal(str(d["amount"])),
                donor_id=d["donor_id"],
                donation_type=d["donation_type"],
                payment_method=d.get("payment_method"),
                campaign=d.get("campaign"),
                appeal=d.get("appeal"),
                is_anonymous=d.get("is_anonymous", False),
                acknowledgement_sent=d.get("acknowledgement_sent", False),
                tax_deductible=d.get("tax_deductible", True),
                notes=d.get("notes"),
                receipt_number=d["receipt_number"],
                created_at=datetime.fromisoformat(d["created_at"].iso_format()),
            )
        )
    return donations


async def create_grant(session: AsyncSession, user_id: str, grant: GrantCreate) -> GrantInDB:
    """Create grant"""
    grant_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    grant_code = f"GRT-{datetime.now(timezone.utc).strftime('%Y%m')}-{grant_id[:6]}"

    query = """
    MATCH (u:User {id: $user_id}), (f:NPOFund {id: $fund_id})
    WHERE $book_id IS NULL OR f.book_id = $book_id
    CREATE (g:Grant {
        id: $id,
        book_id: $book_id,
        grant_code: $grant_code,
        grant_name: $grant_name,
        grantor_name: $grantor_name,
        grant_type: $grant_type,
        status: $status,
        application_date: $application_date,
        approval_date: $approval_date,
        start_date: $start_date,
        end_date: $end_date,
        amount_awarded: toFloat($amount_awarded),
        amount_received: toFloat($amount_received),
        amount_spent: toFloat($amount_spent),
        currency: $currency,
        purpose: $purpose,
        restrictions: $restrictions,
        reporting_requirements: $reporting_requirements,
        next_report_due: $next_report_due,
        performance_indicators: $performance_indicators,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_GRANT]->(g)
    CREATE (g)-[:GRANT_IN_FUND]->(f)
    RETURN g
    """
    params = {
        "id": grant_id,
        "user_id": user_id,
        "fund_id": grant.fund_id,
        "grant_code": grant_code,
        "grant_name": grant.grant_name,
        "grantor_name": grant.grantor_name,
        "grant_type": grant.grant_type,
        "status": grant.status.value,
        "application_date": grant.application_date.isoformat() if grant.application_date else None,
        "approval_date": grant.approval_date.isoformat() if grant.approval_date else None,
        "start_date": grant.start_date.isoformat() if grant.start_date else None,
        "end_date": grant.end_date.isoformat() if grant.end_date else None,
        "amount_awarded": float(grant.amount_awarded),
        "amount_received": float(grant.amount_received),
        "amount_spent": float(grant.amount_spent),
        "currency": grant.currency,
        "purpose": grant.purpose,
        "restrictions": grant.restrictions,
        "reporting_requirements": grant.reporting_requirements,
        "next_report_due": None,
        "performance_indicators": "{}",
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    if not record:
        raise NotFoundError(detail=f"Fund {grant.fund_id} not found", code="FUND_NOT_FOUND")
    g = record["g"]

    return GrantInDB(
        id=g["id"],
        user_id=user_id,
        grant_name=g["grant_name"],
        grantor_name=g["grantor_name"],
        grant_type=g["grant_type"],
        status=GrantStatus(g["status"]),
        application_date=(
            datetime.fromisoformat(g["application_date"].iso_format()).date() if g.get("application_date") else None
        ),
        approval_date=(
            datetime.fromisoformat(g["approval_date"].iso_format()).date() if g.get("approval_date") else None
        ),
        start_date=datetime.fromisoformat(g["start_date"].iso_format()).date() if g.get("start_date") else None,
        end_date=datetime.fromisoformat(g["end_date"].iso_format()).date() if g.get("end_date") else None,
        amount_awarded=Decimal(str(g["amount_awarded"])),
        amount_received=Decimal(str(g["amount_received"])),
        amount_spent=Decimal(str(g["amount_spent"])),
        currency=g["currency"],
        purpose=g["purpose"],
        restrictions=g.get("restrictions"),
        reporting_requirements=g.get("reporting_requirements"),
        fund_id=grant.fund_id,
        grant_code=g["grant_code"],
        created_at=datetime.fromisoformat(g["created_at"].iso_format()),
    )


async def get_grants(session: AsyncSession, user_id: str, status: Optional[GrantStatus] = None) -> List[GrantInDB]:
    """Get all grants, optionally filtered by status"""
    status_filter = "AND g.status = $status" if status else ""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_GRANT]->(g:Grant)-[:GRANT_IN_FUND]->(f:NPOFund)
    WHERE true {status_filter} AND ($book_id IS NULL OR g.book_id = $book_id)
    RETURN g, f.id as fund_id
    ORDER BY g.grant_code
    """
    params = {"user_id": user_id}
    if status:
        params["status"] = status.value

    result = await _run(session, query, params)
    grants = []
    async for record in result:
        g = record["g"]
        grants.append(
            GrantInDB(
                id=g["id"],
                user_id=user_id,
                grant_name=g["grant_name"],
                grantor_name=g["grantor_name"],
                grant_type=g["grant_type"],
                status=GrantStatus(g["status"]),
                application_date=(
                    datetime.fromisoformat(g["application_date"].iso_format()).date()
                    if g.get("application_date")
                    else None
                ),
                approval_date=(
                    datetime.fromisoformat(g["approval_date"].iso_format()).date() if g.get("approval_date") else None
                ),
                start_date=datetime.fromisoformat(g["start_date"].iso_format()).date() if g.get("start_date") else None,
                end_date=datetime.fromisoformat(g["end_date"].iso_format()).date() if g.get("end_date") else None,
                amount_awarded=Decimal(str(g["amount_awarded"])),
                amount_received=Decimal(str(g["amount_received"])),
                amount_spent=Decimal(str(g["amount_spent"])),
                currency=g["currency"],
                purpose=g["purpose"],
                restrictions=g.get("restrictions"),
                reporting_requirements=g.get("reporting_requirements"),
                fund_id=record["fund_id"],
                grant_code=g["grant_code"],
                created_at=datetime.fromisoformat(g["created_at"].iso_format()),
            )
        )
    return grants


# =============================================================================
# PROJECT AND PROGRAM CRUD
# =============================================================================


async def create_project(session: AsyncSession, user_id: str, project: ProjectCreate) -> ProjectInDB:
    """Create NPO project"""
    project_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (p:Project {
        id: $id,
        book_id: $book_id,
        project_name: $project_name,
        project_code: $project_code,
        description: $description,
        status: $status,
        start_date: $start_date,
        end_date: $end_date,
        total_budget: toFloat($total_budget),
        spent_amount: toFloat($spent_amount),
        funding_source: $funding_source,
        fund_id: $fund_id,
        program_id: $program_id,
        location: $location,
        target_beneficiaries: $target_beneficiaries,
        actual_beneficiaries: $actual_beneficiaries,
        completion_percent: toFloat($completion_percent),
        key_milestones: $key_milestones,
        risks: $risks,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_PROJECT]->(p)
    RETURN p
    """
    params = {
        "id": project_id,
        "user_id": user_id,
        "project_name": project.project_name,
        "project_code": project.project_code,
        "description": project.description,
        "status": project.status.value,
        "start_date": project.start_date.isoformat() if project.start_date else None,
        "end_date": project.end_date.isoformat() if project.end_date else None,
        "total_budget": float(project.total_budget),
        "spent_amount": float(project.spent_amount),
        "funding_source": project.funding_source,
        "fund_id": project.fund_id,
        "program_id": project.program_id,
        "location": project.location,
        "target_beneficiaries": project.target_beneficiaries,
        "actual_beneficiaries": project.actual_beneficiaries,
        "completion_percent": 0.0,
        "key_milestones": "[]",
        "risks": "[]",
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    p = record["p"]

    return ProjectInDB(
        id=p["id"],
        user_id=user_id,
        project_name=p["project_name"],
        project_code=p["project_code"],
        description=p["description"],
        status=ProjectStatus(p["status"]),
        start_date=datetime.fromisoformat(p["start_date"].iso_format()).date() if p.get("start_date") else None,
        end_date=datetime.fromisoformat(p["end_date"].iso_format()).date() if p.get("end_date") else None,
        total_budget=Decimal(str(p["total_budget"])),
        spent_amount=Decimal(str(p["spent_amount"])),
        funding_source=p.get("funding_source"),
        fund_id=p.get("fund_id"),
        program_id=p.get("program_id"),
        location=p.get("location"),
        target_beneficiaries=p.get("target_beneficiaries"),
        actual_beneficiaries=p.get("actual_beneficiaries"),
        completion_percent=Decimal(str(p["completion_percent"])),
        created_at=datetime.fromisoformat(p["created_at"].iso_format()),
    )


async def get_projects(
    session: AsyncSession, user_id: str, status: Optional[ProjectStatus] = None
) -> List[ProjectInDB]:
    """Get all projects"""
    status_filter = "AND p.status = $status" if status else ""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PROJECT]->(p:Project)
    WHERE true {status_filter} AND ($book_id IS NULL OR p.book_id = $book_id)
    RETURN p
    ORDER BY p.project_code
    """
    params = {"user_id": user_id}
    if status:
        params["status"] = status.value

    result = await _run(session, query, params)
    projects = []
    async for record in result:
        p = record["p"]
        projects.append(
            ProjectInDB(
                id=p["id"],
                user_id=user_id,
                project_name=p["project_name"],
                project_code=p["project_code"],
                description=p["description"],
                status=ProjectStatus(p["status"]),
                start_date=datetime.fromisoformat(p["start_date"].iso_format()).date() if p.get("start_date") else None,
                end_date=datetime.fromisoformat(p["end_date"].iso_format()).date() if p.get("end_date") else None,
                total_budget=Decimal(str(p["total_budget"])),
                spent_amount=Decimal(str(p["spent_amount"])),
                funding_source=p.get("funding_source"),
                fund_id=p.get("fund_id"),
                program_id=p.get("program_id"),
                location=p.get("location"),
                target_beneficiaries=p.get("target_beneficiaries"),
                actual_beneficiaries=p.get("actual_beneficiaries"),
                completion_percent=Decimal(str(p["completion_percent"])),
                created_at=datetime.fromisoformat(p["created_at"].iso_format()),
            )
        )
    return projects


async def create_program(session: AsyncSession, user_id: str, program: ProgramCreate) -> ProgramInDB:
    """Create NPO program"""
    program_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (p:Program {
        id: $id,
        book_id: $book_id,
        program_name: $program_name,
        program_code: $program_code,
        description: $description,
        mission_alignment: $mission_alignment,
        budget_amount: toFloat($budget_amount),
        spent_amount: toFloat($spent_amount),
        director: $director,
        start_date: $start_date,
        status: $status,
        program_type: $program_type,
        beneficiaries_served: $beneficiaries_served,
        outcomes_achieved: $outcomes_achieved,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_PROGRAM]->(p)
    RETURN p
    """
    params = {
        "id": program_id,
        "user_id": user_id,
        "program_name": program.program_name,
        "program_code": program.program_code,
        "description": program.description,
        "mission_alignment": program.mission_alignment,
        "budget_amount": float(program.budget_amount),
        "spent_amount": float(program.spent_amount),
        "director": program.director,
        "start_date": program.start_date.isoformat() if program.start_date else None,
        "status": program.status,
        "program_type": program.program_type if hasattr(program, "program_type") else "general",
        "beneficiaries_served": None,
        "outcomes_achieved": "[]",
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    p = record["p"]

    return ProgramInDB(
        id=p["id"],
        user_id=user_id,
        program_name=p["program_name"],
        program_code=p["program_code"],
        description=p["description"],
        mission_alignment=p["mission_alignment"],
        budget_amount=Decimal(str(p["budget_amount"])),
        spent_amount=Decimal(str(p["spent_amount"])),
        director=p.get("director"),
        start_date=datetime.fromisoformat(p["start_date"].iso_format()).date() if p.get("start_date") else None,
        status=p["status"],
        program_type=p["program_type"],
        beneficiaries_served=p.get("beneficiaries_served"),
        outcomes_achieved=[],
        created_at=datetime.fromisoformat(p["created_at"].iso_format()),
    )


async def get_programs(session: AsyncSession, user_id: str) -> List[ProgramInDB]:
    """Get all programs"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_PROGRAM]->(p:Program)
    WHERE $book_id IS NULL OR p.book_id = $book_id
    RETURN p
    ORDER BY p.program_code
    """
    result = await _run(session, query, user_id=user_id)
    programs = []
    async for record in result:
        p = record["p"]
        programs.append(
            ProgramInDB(
                id=p["id"],
                user_id=user_id,
                program_name=p["program_name"],
                program_code=p["program_code"],
                description=p["description"],
                mission_alignment=p["mission_alignment"],
                budget_amount=Decimal(str(p["budget_amount"])),
                spent_amount=Decimal(str(p["spent_amount"])),
                director=p.get("director"),
                start_date=datetime.fromisoformat(p["start_date"].iso_format()).date() if p.get("start_date") else None,
                status=p["status"],
                program_type=p.get("program_type", "general"),
                beneficiaries_served=p.get("beneficiaries_served"),
                outcomes_achieved=[],
                created_at=datetime.fromisoformat(p["created_at"].iso_format()),
            )
        )
    return programs


# =============================================================================
# DONOR CRUD
# =============================================================================


async def create_donor(session: AsyncSession, user_id: str, donor: DonorCreate) -> DonorInDB:
    """Create donor"""
    donor_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    donor_code = f"DR-{datetime.now(timezone.utc).strftime('%Y%m')}-{donor_id[:6]}"

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (d:Donor {
        id: $id,
        book_id: $book_id,
        donor_code: $donor_code,
        donor_name: $donor_name,
        donor_type: $donor_type,
        email: $email,
        phone: $phone,
        address: $address,
        tax_id: $tax_id,
        first_donation_date: $first_donation_date,
        last_donation_date: $last_donation_date,
        lifetime_donations: toFloat($lifetime_donations),
        notes: $notes,
        communication_preferences: $communication_preferences,
        stewardship_tier: $stewardship_tier,
        preferred_fund: $preferred_fund,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_DONOR]->(d)
    RETURN d
    """
    params = {
        "id": donor_id,
        "user_id": user_id,
        "donor_code": donor_code,
        "donor_name": donor.donor_name,
        "donor_type": donor.donor_type,
        "email": donor.email,
        "phone": donor.phone,
        "address": donor.address,
        "tax_id": donor.tax_id,
        "first_donation_date": donor.first_donation_date.isoformat() if donor.first_donation_date else None,
        "last_donation_date": donor.last_donation_date.isoformat() if donor.last_donation_date else None,
        "lifetime_donations": float(donor.lifetime_donations),
        "notes": donor.notes,
        "communication_preferences": "{}",
        "stewardship_tier": None,
        "preferred_fund": None,
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    d = record["d"]

    return DonorInDB(
        id=d["id"],
        user_id=user_id,
        donor_name=d["donor_name"],
        donor_type=d["donor_type"],
        email=d.get("email"),
        phone=d.get("phone"),
        address=d.get("address"),
        tax_id=d.get("tax_id"),
        first_donation_date=(
            datetime.fromisoformat(d["first_donation_date"].iso_format()).date()
            if d.get("first_donation_date")
            else None
        ),
        last_donation_date=(
            datetime.fromisoformat(d["last_donation_date"].iso_format()).date() if d.get("last_donation_date") else None
        ),
        lifetime_donations=Decimal(str(d["lifetime_donations"])),
        notes=d.get("notes"),
        donor_code=d["donor_code"],
        created_at=datetime.fromisoformat(d["created_at"].iso_format()),
    )


async def get_donors(session: AsyncSession, user_id: str) -> List[DonorInDB]:
    """Get all donors"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_DONOR]->(d:Donor)
    WHERE $book_id IS NULL OR d.book_id = $book_id
    RETURN d
    ORDER BY d.donor_name
    """
    result = await _run(session, query, user_id=user_id)
    donors = []
    async for record in result:
        d = record["d"]
        donors.append(
            DonorInDB(
                id=d["id"],
                user_id=user_id,
                donor_name=d["donor_name"],
                donor_type=d["donor_type"],
                email=d.get("email"),
                phone=d.get("phone"),
                address=d.get("address"),
                tax_id=d.get("tax_id"),
                first_donation_date=(
                    datetime.fromisoformat(d["first_donation_date"].iso_format()).date()
                    if d.get("first_donation_date")
                    else None
                ),
                last_donation_date=(
                    datetime.fromisoformat(d["last_donation_date"].iso_format()).date()
                    if d.get("last_donation_date")
                    else None
                ),
                lifetime_donations=Decimal(str(d["lifetime_donations"])),
                notes=d.get("notes"),
                donor_code=d["donor_code"],
                created_at=datetime.fromisoformat(d["created_at"].iso_format()),
            )
        )
    return donors


# =============================================================================
# BUDGET CRUD
# =============================================================================


async def create_budget(session: AsyncSession, user_id: str, budget: BudgetCreate) -> BudgetInDB:
    """Create budget"""
    budget_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    budget_code = f"BUD-{budget.fiscal_year}-{budget_id[:6]}"

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (b:Budget {
        id: $id,
        book_id: $book_id,
        budget_code: $budget_code,
        budget_name: $budget_name,
        fiscal_year: $fiscal_year,
        period_start: date($period_start),
        period_end: date($period_end),
        status: $status,
        total_budget: toFloat($total_budget),
        fund_id: $fund_id,
        project_id: $project_id,
        program_id: $program_id,
        total_allocated: toFloat($total_allocated),
        total_spent: toFloat($total_spent),
        remaining_balance: toFloat($remaining_balance),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_BUDGET]->(b)
    RETURN b
    """
    params = {
        "id": budget_id,
        "user_id": user_id,
        "budget_code": budget_code,
        "budget_name": budget.budget_name,
        "fiscal_year": budget.fiscal_year,
        "period_start": budget.period_start.isoformat(),
        "period_end": budget.period_end.isoformat(),
        "status": budget.status.value,
        "total_budget": float(budget.total_budget),
        "fund_id": budget.fund_id,
        "project_id": budget.project_id,
        "program_id": budget.program_id,
        "total_allocated": 0.0,
        "total_spent": 0.0,
        "remaining_balance": float(budget.total_budget),
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    b = record["b"]

    return BudgetInDB(
        id=b["id"],
        user_id=user_id,
        budget_name=b["budget_name"],
        fiscal_year=b["fiscal_year"],
        period_start=datetime.fromisoformat(b["period_start"].iso_format()).date(),
        period_end=datetime.fromisoformat(b["period_end"].iso_format()).date(),
        status=BudgetStatus(b["status"]),
        total_budget=Decimal(str(b["total_budget"])),
        fund_id=b.get("fund_id"),
        project_id=b.get("project_id"),
        program_id=b.get("program_id"),
        budget_code=b["budget_code"],
        total_allocated=Decimal(str(b["total_allocated"])),
        total_spent=Decimal(str(b["total_spent"])),
        remaining_balance=Decimal(str(b["remaining_balance"])),
        created_at=datetime.fromisoformat(b["created_at"].iso_format()),
    )


async def create_budget_line(
    session: AsyncSession, user_id: str, budget_id: str, line: BudgetLineCreate
) -> BudgetLineInDB:
    """Create budget line item"""
    line_id = str(uuid.uuid4())

    # Get budget to calculate line number
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_BUDGET]->(b:Budget {id: $budget_id})
    OPTIONAL MATCH (b)-[:HAS_LINE]->(l:BudgetLine)
    WHERE $book_id IS NULL OR b.book_id = $book_id
    RETURN count(l) as line_count
    """
    result = await _run(session, query, user_id=user_id, budget_id=budget_id)
    record = await result.single()
    if not record:
        raise NotFoundError(detail=f"Budget {budget_id} not found", code="BUDGET_NOT_FOUND")
    line_number = (record["line_count"] or 0) + 1

    variance = line.budgeted_amount - line.spent_amount
    variance_percent = (variance / line.budgeted_amount * 100) if line.budgeted_amount != 0 else Decimal("0.00")

    query = """
    MATCH (u:User {id: $user_id}), (b:Budget {id: $budget_id})
    WHERE $book_id IS NULL OR b.book_id = $book_id
    CREATE (l:BudgetLine {
        id: $id,
        book_id: $book_id,
        line_number: $line_number,
        line_description: $line_description,
        category: $category,
        budgeted_amount: toFloat($budgeted_amount),
        allocated_amount: toFloat($allocated_amount),
        spent_amount: toFloat($spent_amount),
        variance: toFloat($variance),
        variance_percent: toFloat($variance_percent),
        cost_allocation_method: $cost_allocation_method,
        notes: $notes,
        is_over_budget: $is_over_budget
    })
    CREATE (u)-[:OWNS_BUDGET_LINE]->(l)
    CREATE (b)-[:HAS_LINE]->(l)
    RETURN l
    """
    params = {
        "id": line_id,
        "user_id": user_id,
        "budget_id": budget_id,
        "line_number": line_number,
        "line_description": line.line_description,
        "category": line.category,
        "budgeted_amount": float(line.budgeted_amount),
        "allocated_amount": float(line.allocated_amount),
        "spent_amount": float(line.spent_amount),
        "variance": float(variance),
        "variance_percent": float(variance_percent),
        "cost_allocation_method": line.cost_allocation_method,
        "notes": line.notes,
        "is_over_budget": line.spent_amount > line.budgeted_amount,
    }
    result = await _run(session, query, params)
    record = await result.single()
    if not record:
        raise NotFoundError(detail=f"Budget {budget_id} not found", code="BUDGET_NOT_FOUND")
    l = record["l"]

    # Update budget totals
    update_query = """
    MATCH (b:Budget {id: $budget_id})
    WHERE $book_id IS NULL OR b.book_id = $book_id
    SET b.total_allocated = b.total_allocated + toFloat($allocated),
        b.remaining_balance = b.total_budget - (b.total_allocated + toFloat($spent))
    """
    await _run(
        session,
        update_query,
        budget_id=budget_id,
        allocated=float(line.allocated_amount),
        spent=float(line.spent_amount),
    )

    return BudgetLineInDB(
        id=l["id"],
        budget_id=budget_id,
        line_number=l["line_number"],
        line_description=l["line_description"],
        category=l["category"],
        budgeted_amount=Decimal(str(l["budgeted_amount"])),
        allocated_amount=Decimal(str(l["allocated_amount"])),
        spent_amount=Decimal(str(l["spent_amount"])),
        variance=Decimal(str(l["variance"])),
        variance_percent=Decimal(str(l["variance_percent"])),
        is_over_budget=l["is_over_budget"],
    )


async def get_budgets(session: AsyncSession, user_id: str, fiscal_year: Optional[str] = None) -> List[BudgetInDB]:
    """Get all budgets"""
    year_filter = "AND b.fiscal_year = $fiscal_year" if fiscal_year else ""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_BUDGET]->(b:Budget)
    WHERE true {year_filter} AND ($book_id IS NULL OR b.book_id = $book_id)
    RETURN b
    ORDER BY b.fiscal_year DESC, b.budget_name
    """
    params = {"user_id": user_id}
    if fiscal_year:
        params["fiscal_year"] = fiscal_year

    result = await _run(session, query, params)
    budgets = []
    async for record in result:
        b = record["b"]
        budgets.append(
            BudgetInDB(
                id=b["id"],
                user_id=user_id,
                budget_name=b["budget_name"],
                fiscal_year=b["fiscal_year"],
                period_start=datetime.fromisoformat(b["period_start"].iso_format()).date(),
                period_end=datetime.fromisoformat(b["period_end"].iso_format()).date(),
                status=BudgetStatus(b["status"]),
                total_budget=Decimal(str(b["total_budget"])),
                fund_id=b.get("fund_id"),
                project_id=b.get("project_id"),
                program_id=b.get("program_id"),
                budget_code=b["budget_code"],
                total_allocated=Decimal(str(b["total_allocated"])),
                total_spent=Decimal(str(b["total_spent"])),
                remaining_balance=Decimal(str(b["remaining_balance"])),
                created_at=datetime.fromisoformat(b["created_at"].iso_format()),
            )
        )
    return budgets


# =============================================================================
# COMPLIANCE CRUD
# =============================================================================


async def create_internal_control(
    session: AsyncSession, user_id: str, control: InternalControlCreate
) -> InternalControlInDB:
    """Create internal control"""
    control_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (c:InternalControl {
        id: $id,
        book_id: $book_id,
        control_name: $control_name,
        control_type: $control_type,
        category: $category,
        description: $description,
        implemented_date: date($implemented_date),
        responsible_person: $responsible_person,
        frequency: $frequency,
        last_reviewed: $last_reviewed,
        status: $status,
        notes: $notes,
        effectiveness_rating: $effectiveness_rating,
        deficiencies: $deficiencies,
        remediation_date: $remediation_date,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_INTERNAL_CONTROL]->(c)
    RETURN c
    """
    params = {
        "id": control_id,
        "user_id": user_id,
        "control_name": control.control_name,
        "control_type": control.control_type,
        "category": control.category,
        "description": control.description,
        "implemented_date": control.implemented_date.isoformat(),
        "responsible_person": control.responsible_person,
        "frequency": control.frequency,
        "last_reviewed": control.last_reviewed.isoformat() if control.last_reviewed else None,
        "status": control.status,
        "notes": control.notes,
        "effectiveness_rating": control.effectiveness_rating,
        "deficiencies": control.deficiencies,
        "remediation_date": control.remediation_date.isoformat() if control.remediation_date else None,
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    c = record["c"]

    return InternalControlInDB(
        id=c["id"],
        user_id=user_id,
        control_name=c["control_name"],
        control_type=c["control_type"],
        category=c["category"],
        description=c["description"],
        implemented_date=datetime.fromisoformat(c["implemented_date"].iso_format()).date(),
        responsible_person=c["responsible_person"],
        frequency=c["frequency"],
        last_reviewed=(
            datetime.fromisoformat(c["last_reviewed"].iso_format()).date() if c.get("last_reviewed") else None
        ),
        status=c["status"],
        notes=c.get("notes"),
        effectiveness_rating=c.get("effectiveness_rating"),
        deficiencies=c.get("deficiencies"),
        remediation_date=(
            datetime.fromisoformat(c["remediation_date"].iso_format()).date() if c.get("remediation_date") else None
        ),
        created_at=datetime.fromisoformat(c["created_at"].iso_format()),
    )


async def get_internal_controls(session: AsyncSession, user_id: str) -> List[InternalControlInDB]:
    """Get all internal controls"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_INTERNAL_CONTROL]->(c:InternalControl)
    WHERE $book_id IS NULL OR c.book_id = $book_id
    RETURN c
    ORDER BY c.control_name
    """
    result = await _run(session, query, user_id=user_id)
    controls = []
    async for record in result:
        c = record["c"]
        controls.append(
            InternalControlInDB(
                id=c["id"],
                user_id=user_id,
                control_name=c["control_name"],
                control_type=c["control_type"],
                category=c["category"],
                description=c["description"],
                implemented_date=datetime.fromisoformat(c["implemented_date"].iso_format()).date(),
                responsible_person=c["responsible_person"],
                frequency=c["frequency"],
                last_reviewed=(
                    datetime.fromisoformat(c["last_reviewed"].iso_format()).date() if c.get("last_reviewed") else None
                ),
                status=c["status"],
                notes=c.get("notes"),
                effectiveness_rating=c.get("effectiveness_rating"),
                deficiencies=c.get("deficiencies"),
                remediation_date=(
                    datetime.fromisoformat(c["remediation_date"].iso_format()).date()
                    if c.get("remediation_date")
                    else None
                ),
                created_at=datetime.fromisoformat(c["created_at"].iso_format()),
            )
        )
    return controls


async def create_audit_report(session: AsyncSession, user_id: str, audit: AuditReportCreate) -> AuditReportInDB:
    """Create audit report"""
    audit_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    report_number = f"AUD-{datetime.now(timezone.utc).strftime('%Y%m%d')}-{audit_id[:6]}"

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (a:AuditReport {
        id: $id,
        book_id: $book_id,
        report_number: $report_number,
        audit_type: $audit_type,
        audit_name: $audit_name,
        audit_period_start: date($audit_period_start),
        audit_period_end: date($audit_period_end),
        auditor_name: $auditor_name,
        auditor_firm: $auditor_firm,
        start_date: date($start_date),
        end_date: $end_date,
        status: $status,
        findings: $findings,
        recommendations: $recommendations,
        overall_opinion: $overall_opinion,
        compliance_status: $compliance_status,
        issues_count: $issues_count,
        significant_findings: $significant_findings,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_AUDIT_REPORT]->(a)
    RETURN a
    """
    params = {
        "id": audit_id,
        "user_id": user_id,
        "report_number": report_number,
        "audit_type": audit.audit_type.value,
        "audit_name": audit.audit_name,
        "audit_period_start": audit.audit_period_start.isoformat(),
        "audit_period_end": audit.audit_period_end.isoformat(),
        "auditor_name": audit.auditor_name,
        "auditor_firm": audit.auditor_firm,
        "start_date": audit.start_date.isoformat(),
        "end_date": audit.end_date.isoformat() if audit.end_date else None,
        "status": audit.status,
        "findings": str(audit.findings) if audit.findings else "[]",
        "recommendations": str(audit.recommendations) if audit.recommendations else "[]",
        "overall_opinion": audit.overall_opinion,
        "compliance_status": audit.compliance_status.value,
        "issues_count": len(audit.findings) if audit.findings else 0,
        "significant_findings": 0,
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    a = record["a"]

    return AuditReportInDB(
        id=a["id"],
        user_id=user_id,
        audit_name=a["audit_name"],
        audit_type=a["audit_type"],
        audit_period_start=datetime.fromisoformat(a["audit_period_start"].iso_format()).date(),
        audit_period_end=datetime.fromisoformat(a["audit_period_end"].iso_format()).date(),
        auditor_name=a["auditor_name"],
        auditor_firm=a.get("auditor_firm"),
        start_date=datetime.fromisoformat(a["start_date"].iso_format()).date(),
        end_date=datetime.fromisoformat(a["end_date"].iso_format()).date() if a.get("end_date") else None,
        status=a["status"],
        findings=[],
        recommendations=[],
        overall_opinion=a.get("overall_opinion"),
        compliance_status=a["compliance_status"],
        report_number=a["report_number"],
        issues_count=a["issues_count"],
        significant_findings=a["significant_findings"],
        created_at=datetime.fromisoformat(a["created_at"].iso_format()),
    )


async def get_audit_reports(session: AsyncSession, user_id: str) -> List[AuditReportInDB]:
    """Get all audit reports"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_AUDIT_REPORT]->(a:AuditReport)
    WHERE $book_id IS NULL OR a.book_id = $book_id
    RETURN a
    ORDER BY a.start_date DESC
    """
    result = await _run(session, query, user_id=user_id)
    reports = []
    async for record in result:
        a = record["a"]
        reports.append(
            AuditReportInDB(
                id=a["id"],
                user_id=user_id,
                audit_name=a["audit_name"],
                audit_type=a["audit_type"],
                audit_period_start=datetime.fromisoformat(a["audit_period_start"].iso_format()).date(),
                audit_period_end=datetime.fromisoformat(a["audit_period_end"].iso_format()).date(),
                auditor_name=a["auditor_name"],
                auditor_firm=a.get("auditor_firm"),
                start_date=datetime.fromisoformat(a["start_date"].iso_format()).date(),
                end_date=datetime.fromisoformat(a["end_date"].iso_format()).date() if a.get("end_date") else None,
                status=a["status"],
                findings=[],
                recommendations=[],
                overall_opinion=a.get("overall_opinion"),
                compliance_status=a["compliance_status"],
                report_number=a["report_number"],
                issues_count=a["issues_count"],
                significant_findings=a["significant_findings"],
                created_at=datetime.fromisoformat(a["created_at"].iso_format()),
            )
        )
    return reports


# =============================================================================
# PERFORMANCE AND IMPACT CRUD
# =============================================================================


async def create_program_metric(
    session: AsyncSession, user_id: str, metric: ProgramMetricCreate
) -> ProgramMetricCreate:
    """Create program metric"""
    metric_id = str(uuid.uuid4())

    variance = metric.actual_value - metric.target_value

    query = """
    MATCH (u:User {id: $user_id}), (p:Program {id: $program_id})
    WHERE $book_id IS NULL OR p.book_id = $book_id
    CREATE (m:ProgramMetric {
        id: $id,
        book_id: $book_id,
        metric_name: $metric_name,
        metric_type: $metric_type,
        measurement_unit: $measurement_unit,
        target_value: toFloat($target_value),
        actual_value: toFloat($actual_value),
        variance: toFloat($variance),
        calculation_date: date($calculation_date),
        methodology: $methodology,
        notes: $notes
    })
    CREATE (u)-[:OWNS_PROGRAM_METRIC]->(m)
    CREATE (m)-[:METRIC_FOR_PROGRAM]->(p)
    RETURN m
    """
    params = {
        "id": metric_id,
        "user_id": user_id,
        "program_id": metric.program_id,
        "metric_name": metric.metric_name,
        "metric_type": metric.metric_type,
        "measurement_unit": metric.measurement_unit,
        "target_value": float(metric.target_value),
        "actual_value": float(metric.actual_value),
        "variance": float(variance),
        "calculation_date": metric.calculation_date.isoformat(),
        "methodology": metric.methodology,
        "notes": metric.notes,
    }
    await _run(session, query, params)
    return metric


async def create_impact_measurement(
    session: AsyncSession, user_id: str, measurement: ImpactMeasurementCreate
) -> ImpactMeasurementCreate:
    """Create impact measurement"""
    measurement_id = str(uuid.uuid4())

    change_value = measurement.current_value - (measurement.baseline_value or Decimal("0.00"))
    change_percent = (
        (change_value / measurement.baseline_value * 100)
        if measurement.baseline_value and measurement.baseline_value != 0
        else Decimal("0.00")
    )

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (m:ImpactMeasurement {
        id: $id,
        book_id: $book_id,
        measurement_name: $measurement_name,
        program_id: $program_id,
        project_id: $project_id,
        impact_area: $impact_area,
        measurement_date: date($measurement_date),
        beneficiaries_count: $beneficiaries_count,
        measurement_type: $measurement_type,
        baseline_value: toFloat($baseline_value),
        current_value: toFloat($current_value),
        change_value: toFloat($change_value),
        change_percent: toFloat($change_percent),
        methodology: $methodology
    })
    CREATE (u)-[:OWNS_IMPACT_MEASUREMENT]->(m)
    RETURN m
    """
    params = {
        "id": measurement_id,
        "user_id": user_id,
        "measurement_name": measurement.measurement_name,
        "program_id": measurement.program_id,
        "project_id": measurement.project_id,
        "impact_area": measurement.impact_area,
        "measurement_date": measurement.measurement_date.isoformat(),
        "beneficiaries_count": measurement.beneficiaries_count,
        "measurement_type": measurement.measurement_type,
        "baseline_value": float(measurement.baseline_value or Decimal("0.00")),
        "current_value": float(measurement.current_value),
        "change_value": float(change_value),
        "change_percent": float(change_percent),
        "methodology": measurement.methodology,
    }
    await _run(session, query, params)
    return measurement


async def create_volunteer_record(
    session: AsyncSession, user_id: str, record: VolunteerRecordCreate
) -> VolunteerRecordInDB:
    """Create volunteer record"""
    record_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    # Calculate value of service
    hourly_rate = record.hourly_rate_value or Decimal("25.00")  # Default $25/hour
    value_of_service = record.hours_contributed * hourly_rate

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (v:VolunteerRecord {
        id: $id,
        book_id: $book_id,
        volunteer_name: $volunteer_name,
        volunteer_id: $volunteer_id,
        activity_date: date($activity_date),
        hours_contributed: toFloat($hours_contributed),
        activity_type: $activity_type,
        program_id: $program_id,
        project_id: $project_id,
        supervisor: $supervisor,
        description: $description,
        is_skilled: $is_skilled,
        hourly_rate_value: toFloat($hourly_rate_value),
        value_of_service: toFloat($value_of_service),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_VOLUNTEER_RECORD]->(v)
    RETURN v
    """
    params = {
        "id": record_id,
        "user_id": user_id,
        "volunteer_name": record.volunteer_name,
        "volunteer_id": record.volunteer_id,
        "activity_date": record.activity_date.isoformat(),
        "hours_contributed": float(record.hours_contributed),
        "activity_type": record.activity_type,
        "program_id": record.program_id,
        "project_id": record.project_id,
        "supervisor": record.supervisor,
        "description": record.description,
        "is_skilled": record.is_skilled,
        "hourly_rate_value": float(record.hourly_rate_value or Decimal("25.00")),
        "value_of_service": float(value_of_service),
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record_db = result.single()["v"]

    return VolunteerRecordInDB(
        id=record_db["id"],
        user_id=user_id,
        volunteer_name=record_db["volunteer_name"],
        volunteer_id=record_db["volunteer_id"],
        activity_date=datetime.fromisoformat(record_db["activity_date"].iso_format()).date(),
        hours_contributed=Decimal(str(record_db["hours_contributed"])),
        activity_type=record_db["activity_type"],
        program_id=record_db.get("program_id"),
        project_id=record_db.get("project_id"),
        supervisor=record_db.get("supervisor"),
        description=record_db.get("description"),
        is_skilled=record_db["is_skilled"],
        hourly_rate_value=Decimal(str(record_db["hourly_rate_value"])),
        value_of_service=Decimal(str(record_db["value_of_service"])),
        created_at=datetime.fromisoformat(record_db["created_at"].iso_format()),
    )


async def get_volunteer_records(
    session: AsyncSession, user_id: str, start_date=None, end_date=None
) -> List[VolunteerRecordInDB]:
    """Get volunteer records"""
    date_filter = ""
    params = {"user_id": user_id}
    if start_date:
        date_filter += " AND v.activity_date >= date($start_date)"
        params["start_date"] = start_date.isoformat()
    if end_date:
        date_filter += " AND v.activity_date <= date($end_date)"
        params["end_date"] = end_date.isoformat()

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_VOLUNTEER_RECORD]->(v:VolunteerRecord)
    WHERE true {date_filter} AND ($book_id IS NULL OR v.book_id = $book_id)
    RETURN v
    ORDER BY v.activity_date DESC
    """
    result = await _run(session, query, params)
    records = []
    async for record in result:
        v = record["v"]
        records.append(
            VolunteerRecordInDB(
                id=v["id"],
                user_id=user_id,
                volunteer_name=v["volunteer_name"],
                volunteer_id=v["volunteer_id"],
                activity_date=datetime.fromisoformat(v["activity_date"].iso_format()).date(),
                hours_contributed=Decimal(str(v["hours_contributed"])),
                activity_type=v["activity_type"],
                program_id=v.get("program_id"),
                project_id=v.get("project_id"),
                supervisor=v.get("supervisor"),
                description=v.get("description"),
                is_skilled=v["is_skilled"],
                hourly_rate_value=Decimal(str(v["hourly_rate_value"])),
                value_of_service=Decimal(str(v["value_of_service"])),
                created_at=datetime.fromisoformat(v["created_at"].iso_format()),
            )
        )
    return records


# =============================================================================
# FINANCIAL STATEMENTS CRUD
# =============================================================================


async def create_statement_of_activities(
    session: AsyncSession, user_id: str, period_start: date, period_end: date
) -> StatementOfActivitiesInDB:
    """Create Statement of Activities"""
    stmt_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    # Calculate from donations, grants, and transactions
    donations_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_DONATION]->(d:Donation)
    WHERE d.donation_date >= date($period_start) AND d.donation_date <= date($period_end) AND ($book_id IS NULL OR d.book_id = $book_id)
    RETURN sum(d.amount) as total_donations, count(d) as donation_count
    """
    donations_result = await _run(
        session,
        donations_query,
        user_id=user_id,
        period_start=period_start.isoformat(),
        period_end=period_end.isoformat(),
    )
    donations_record = await donations_result.single()
    total_contributions = (
        Decimal(str(donations_record["total_donations"] or 0)) if donations_record else Decimal("0.00")
    )

    # Get grants received
    grants_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_GRANT]->(g:Grant)
    WHERE g.status IN ['active', 'completed'] AND ($book_id IS NULL OR g.book_id = $book_id)
    RETURN sum(g.amount_received) as total_grants
    """
    grants_result = await _run(session, grants_query, user_id=user_id)
    grants_record = await grants_result.single()
    grant_revenue = Decimal(str(grants_record["total_grants"] or 0)) if grants_record else Decimal("0.00")

    # Calculate expenses from fund transactions
    expense_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_NPO_FUND_TRANSACTION]->(t:NPOFundTransaction)
    WHERE t.transaction_type IN ['disbursement', 'expense'] AND ($book_id IS NULL OR t.book_id = $book_id)
    AND t.transaction_date >= date($period_start) AND t.transaction_date <= date($period_end)
    RETURN sum(t.amount) as total_expenses
    """
    expense_result = await _run(
        session,
        expense_query,
        user_id=user_id,
        period_start=period_start.isoformat(),
        period_end=period_end.isoformat(),
    )
    expense_record = await expense_result.single()
    total_expenses = Decimal(str(expense_record["total_expenses"] or 0)) if expense_record else Decimal("0.00")

    # Calculate net assets
    net_assets_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_NET_ASSETS]->(na:NetAssets)
    WHERE $book_id IS NULL OR na.book_id = $book_id
    RETURN na.total_net_assets as total
    ORDER BY na.as_of_date DESC
    LIMIT 1
    """
    net_result = await _run(session, net_assets_query, user_id=user_id)
    net_record = await net_result.single()
    beginning_net = Decimal(str(net_record["total"] or 0)) if net_record else Decimal("0.00")

    change_in_net = total_contributions + grant_revenue - total_expenses
    ending_net = beginning_net + change_in_net

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (s:StatementOfActivities {
        id: $id,
        book_id: $book_id,
        period_start: date($period_start),
        period_end: date($period_end),
        contributions_without_restrictions: toFloat($without),
        contributions_with_restrictions: toFloat($with),
        total_contributions: toFloat($total_contrib),
        program_service_revenue: toFloat($program_rev),
        membership_dues: toFloat($membership),
        fundraising_revenue: toFloat($fundraising),
        investment_income: toFloat($investment),
        other_revenue: toFloat($other),
        total_revenue: toFloat($total_rev),
        program_expenses: toFloat($program_exp),
        administrative_expenses: toFloat($admin_exp),
        fundraising_expenses: toFloat($fund_exp),
        total_expenses: toFloat($total_exp),
        change_in_net_assets: toFloat($change),
        net_assets_beginning: toFloat($beginning),
        net_assets_ending: toFloat($ending),
        lines: $lines,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_STATEMENT_OF_ACTIVITIES]->(s)
    RETURN s
    """
    params = {
        "id": stmt_id,
        "user_id": user_id,
        "period_start": period_start.isoformat(),
        "period_end": period_end.isoformat(),
        "without": float(total_contributions * Decimal("0.7")),  # Assume 70% unrestricted
        "with": float(total_contributions * Decimal("0.3")),  # Assume 30% restricted
        "total_contrib": float(total_contributions),
        "program_rev": float(grant_revenue * Decimal("0.8")),
        "membership": 0.0,
        "fundraising": 0.0,
        "investment": 0.0,
        "other": 0.0,
        "total_rev": float(total_contributions + grant_revenue),
        "program_exp": float(total_expenses * Decimal("0.7")),
        "admin_exp": float(total_expenses * Decimal("0.2")),
        "fund_exp": float(total_expenses * Decimal("0.1")),
        "total_exp": float(total_expenses),
        "change": float(change_in_net),
        "beginning": float(beginning_net),
        "ending": float(ending_net),
        "lines": "[]",
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    s = record["s"]

    return StatementOfActivitiesInDB(
        id=s["id"],
        user_id=user_id,
        period_start=datetime.fromisoformat(s["period_start"].iso_format()).date(),
        period_end=datetime.fromisoformat(s["period_end"].iso_format()).date(),
        contributions_without_restrictions=Decimal(str(s["contributions_without_restrictions"])),
        contributions_with_restrictions=Decimal(str(s["contributions_with_restrictions"])),
        total_contributions=Decimal(str(s["total_contributions"])),
        program_service_revenue=Decimal(str(s["program_service_revenue"])),
        membership_dues=Decimal(str(s["membership_dues"])),
        fundraising_revenue=Decimal(str(s["fundraising_revenue"])),
        investment_income=Decimal(str(s["investment_income"])),
        other_revenue=Decimal(str(s["other_revenue"])),
        total_revenue=Decimal(str(s["total_revenue"])),
        program_expenses=Decimal(str(s["program_expenses"])),
        administrative_expenses=Decimal(str(s["administrative_expenses"])),
        fundraising_expenses=Decimal(str(s["fundraising_expenses"])),
        total_expenses=Decimal(str(s["total_expenses"])),
        change_in_net_assets=Decimal(str(s["change_in_net_assets"])),
        net_assets_beginning=Decimal(str(s["net_assets_beginning"])),
        net_assets_ending=Decimal(str(s["net_assets_ending"])),
        lines=[],
        created_at=datetime.fromisoformat(s["created_at"].iso_format()),
    )


async def create_statement_of_financial_position(
    session: AsyncSession, user_id: str, as_of_date: date
) -> StatementOfFinancialPositionInDB:
    """Create Statement of Financial Position"""
    stmt_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    # Get totals from funds
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_NPO_FUND]->(f:NPOFund)
    WHERE f.status = 'active' AND ($book_id IS NULL OR f.book_id = $book_id)
    RETURN sum(f.current_balance) as total_funds
    """
    result = await _run(session, query, user_id=user_id)
    record = await result.single()
    total_assets = Decimal(str(record["total_funds"] or 0)) if record else Decimal("0.00")

    total_liabilities = Decimal("0.00")  # Simplified
    total_net_assets = total_assets - total_liabilities

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (s:StatementOfFinancialPosition {
        id: $id,
        book_id: $book_id,
        as_of_date: date($as_of_date),
        current_assets: toFloat($current),
        fixed_assets: toFloat($fixed),
        intangible_assets: toFloat($intangible),
        endowment_assets: toFloat($endowment),
        other_assets: toFloat($other),
        total_assets: toFloat($total_assets),
        current_liabilities: toFloat($current_liab),
        long_term_liabilities: toFloat($long_liab),
        total_liabilities: toFloat($total_liab),
        net_assets_without_restrictions: toFloat($without),
        net_assets_with_donor_restrictions: toFloat($with),
        total_net_assets: toFloat($total_net),
        total_liabilities_net_assets: toFloat($total),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_STATEMENT_OF_FP]->(s)
    RETURN s
    """
    params = {
        "id": stmt_id,
        "user_id": user_id,
        "as_of_date": as_of_date.isoformat(),
        "current": float(total_assets * Decimal("0.4")),
        "fixed": float(total_assets * Decimal("0.5")),
        "intangible": 0.0,
        "endowment": float(total_assets * Decimal("0.1")),
        "other": 0.0,
        "total_assets": float(total_assets),
        "current_liab": 0.0,
        "long_liab": 0.0,
        "total_liab": 0.0,
        "without": float(total_net_assets * Decimal("0.6")),
        "with": float(total_net_assets * Decimal("0.4")),
        "total_net": float(total_net_assets),
        "total": float(total_assets),
        "created_at": created_at.isoformat(),
    }
    result = await _run(session, query, params)
    record = await result.single()
    s = record["s"]

    return StatementOfFinancialPositionInDB(
        id=s["id"],
        user_id=user_id,
        as_of_date=datetime.fromisoformat(s["as_of_date"].iso_format()).date(),
        current_assets=Decimal(str(s["current_assets"])),
        fixed_assets=Decimal(str(s["fixed_assets"])),
        intangible_assets=Decimal(str(s["intangible_assets"])),
        endowment_assets=Decimal(str(s["endowment_assets"])),
        other_assets=Decimal(str(s["other_assets"])),
        total_assets=Decimal(str(s["total_assets"])),
        current_liabilities=Decimal(str(s["current_liabilities"])),
        long_term_liabilities=Decimal(str(s["long_term_liabilities"])),
        total_liabilities=Decimal(str(s["total_liabilities"])),
        net_assets_without_restrictions=Decimal(str(s["net_assets_without_restrictions"])),
        net_assets_with_donor_restrictions=Decimal(str(s["net_assets_with_donor_restrictions"])),
        total_net_assets=Decimal(str(s["total_net_assets"])),
        total_liabilities_net_assets=Decimal(str(s["total_liabilities_net_assets"])),
        created_at=datetime.fromisoformat(s["created_at"].iso_format()),
    )


async def get_statement_of_activities(
    session: AsyncSession, user_id: str, period_start: date, period_end: date
) -> Optional[StatementOfActivitiesInDB]:
    """Get Statement of Activities for period"""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_STATEMENT_OF_ACTIVITIES]->(s:StatementOfActivities)
    WHERE s.period_start = date($period_start) AND s.period_end = date($period_end) AND ($book_id IS NULL OR s.book_id = $book_id)
    RETURN s
    """
    result = await _run(
        session, query, user_id=user_id, period_start=period_start.isoformat(), period_end=period_end.isoformat()
    )
    record = await result.single()
    if not record:
        return None
    s = record["s"]
    return StatementOfActivitiesInDB(
        id=s["id"],
        user_id=user_id,
        period_start=datetime.fromisoformat(s["period_start"].iso_format()).date(),
        period_end=datetime.fromisoformat(s["period_end"].iso_format()).date(),
        contributions_without_restrictions=Decimal(str(s["contributions_without_restrictions"])),
        contributions_with_restrictions=Decimal(str(s["contributions_with_restrictions"])),
        total_contributions=Decimal(str(s["total_contributions"])),
        program_service_revenue=Decimal(str(s["program_service_revenue"])),
        membership_dues=Decimal(str(s["membership_dues"])),
        fundraising_revenue=Decimal(str(s["fundraising_revenue"])),
        investment_income=Decimal(str(s["investment_income"])),
        other_revenue=Decimal(str(s["other_revenue"])),
        total_revenue=Decimal(str(s["total_revenue"])),
        program_expenses=Decimal(str(s["program_expenses"])),
        administrative_expenses=Decimal(str(s["administrative_expenses"])),
        fundraising_expenses=Decimal(str(s["fundraising_expenses"])),
        total_expenses=Decimal(str(s["total_expenses"])),
        change_in_net_assets=Decimal(str(s["change_in_net_assets"])),
        net_assets_beginning=Decimal(str(s["net_assets_beginning"])),
        net_assets_ending=Decimal(str(s["net_assets_ending"])),
        lines=[],
        created_at=datetime.fromisoformat(s["created_at"].iso_format()),
    )
