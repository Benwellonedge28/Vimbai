"""
Vimbai Partnership Accounting Service
Partnership-specific accounting including partner management, profit sharing,
capital accounts, and dissolution handling.
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

SERVICE_NAME = "partnership-accounting-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8020"))

# Internal service URLs
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")
BUDGETING_SERVICE_URL = os.getenv("BUDGETING_SERVICE_URL", "http://localhost:8099")
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ADMIN_SERVICE_URL = os.getenv("ADMIN_SERVICE_URL", "http://localhost:8001")

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
    title="Vimbai Partnership Accounting Service",
    description="Partnership-specific accounting including partner management, profit sharing, capital accounts, and dissolution",
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


class PartnershipType(str, Enum):
    GENERAL_PARTNERSHIP = "general_partnership"
    LIMITED_PARTNERSHIP = "limited_partnership"
    LLP = "limited_liability_partnership"
    LP = "limited_partnership"
    FOREIGN_PARTNERSHIP = "foreign_partnership"
    LIMITED_LIABILITY_LIMITED_PARTNERSHIP = "lllp"


class PartnerType(str, Enum):
    GENERAL_PARTNER = "general_partner"
    LIMITED_PARTNER = "limited_partner"
    MANAGING_PARTNER = "managing_partner"
    SLEEPING_PARTNER = "sleeping_partner"


class CapitalAccountType(str, Enum):
    FIXED = "fixed"
    FLUCTUATING = "fluctuating"


class ProfitSharingBasis(str, Enum):
    EQUAL = "equal"
    RATIO = "ratio"
    CAPITAL_BASED = "capital_based"
    ACTIVE_PARTNER = "active_partner"
   guaranteed
    GUARANTEED_SALARY = "guaranteed_salary"
    INTEREST_ON_CAPITAL = "interest_on_capital"


class GoodwillTreatment(str, Enum):
    REVALUATION_METHOD = "revaluation_method"
    GOODWILL_RAISED = "goodwill_raised"
    GOODWILL_RAISED_AND_AMLORITZED = "goodwill_raised_and_amortized"
    GOODWILL_ADJUSTED = "goodwill_adjusted"


class DissolutionType(str, Enum):
    COMPLETE_DISSOLUTION = "complete_dissolution"
    PARTIAL_DISSOLUTION = "partial_dissolution"
    reconstitution
    RECONSTITUTION = "reconstitution"
    CONVERSION = "conversion"


class DissolutionReason(str, Enum):
    EXPIRY_OF_TERM = "expiry_of_term"
    COMPLETION_OF_ADVENTURE = "completion_of_adventure"
    mutual
    MUTUAL_AGREEMENT = "mutual_agreement"
    COMPULSORY_DISSOLUTION = "compulsory_dissolution"
    DEATH_OF_PARTNER = "death_of_partner"
    INSOLVENCY = "insolvency"
    COURT_ORDER = "court_order"


class PaymentType(str, Enum):
    CASH = "cash"
    ASSETS = "assets"
    MIXED = "mixed"


# ============================================================================
# Pydantic Models
# ============================================================================


class Partner(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    partner_type: PartnerType
    profit_sharing_ratio: float = Field(ge=0, le=100)
    capital_contribution: float = Field(ge=0)
    address: Optional[str] = None
    phone: Optional[str] = None
    email: Optional[str] = None
    is_active: bool = True
    admission_date: datetime = Field(default_factory=datetime.utcnow)
    retirement_date: Optional[datetime] = None
    guaranteed_salary: float = Field(default=0, ge=0)
    interest_rate_on_capital: float = Field(default=0, ge=0, le=100)
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)


class PartnershipAgreement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    partnership_type: PartnershipType
    start_date: datetime
    end_date: Optional[datetime] = None
    profit_sharing_basis: ProfitSharingBasis
    capital_account_type: CapitalAccountType
    goodwill_treatment: GoodwillTreatment = GoodwillTreatment.REVALUATION_METHOD
    partners: List[Partner] = []
    additional_terms: Optional[str] = None
    is_active: bool = True
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)


class Drawing(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    partner_id: str
    amount: float = Field(gt=0)
    date: datetime = Field(default_factory=datetime.utcnow)
    description: str
    category: str = "general"  # goods, cash, assets
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CapitalContribution(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    partner_id: str
    amount: float = Field(gt=0)
    date: datetime = Field(default_factory=datetime.utcnow)
    description: str
    type: str = "additional"  # initial, additional, adjustment
    created_at: datetime = Field(default_factory=datetime.utcnow)


class PartnerCurrentAccount(BaseModel):
    partner_id: str
    partner_name: str
    partner_type: PartnerType
    opening_balance: float = 0
    contributions: float = 0
    drawings: float = 0
    profit_share: float = 0
    guaranteed_salary: float = 0
    interest_on_capital: float = 0
    interest_on_drawings: float = 0
    salary: float = 0
    commission: float = 0
    net_effect: float = 0
    closing_balance: float = 0
    entries: List[Dict[str, Any]] = []


class ProfitDistribution(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    period_start: datetime
    period_end: datetime
    total_profit: float
    distribution_date: datetime = Field(default_factory=datetime.utcnow)
    guaranteed_salaries: Dict[str, float] = {}
    interest_on_capital: Dict[str, float] = {}
    profit_shares: Dict[str, float] = {}
    adjustments: Dict[str, float] = {}
    final_distribution: Dict[str, float] = {}
    journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class GoodwillRevaluation(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    old_partners: List[str] = []
    new_partners: List[str] = []
    old_capital: Dict[str, float] = {}
    new_capital: Dict[str, float] = {}
    goodwill_amount: float
    goodwill_journal_entry_id: Optional[str] = None
    revaluation_journal_entry_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)


class PartnerAdmission(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    new_partner: Partner
    old_partners: List[Dict[str, Any]] = []
    admission_type: str = "new_admission"  # new_admission, reconstitution
    goodwill_paid_by_new_partner: float = 0
    old_partners_renunciation: float = 0
    capital adjustment
    new_capital_balances: Dict[str, float] = {}
    journal_entry_ids: List[str] = []
    created_at: datetime = Field(default_factory=datetime.utcnow)


class PartnerRetirement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    retiring_partner_id: str
    remaining_partners: List[str] = []
    retirement_type: str = "normal"  # normal, death, insolvency
    amount_payable: float
    payment_type: PaymentType
    goodwill_amount: float = 0
    old_capital: Dict[str, float] = {}
    new_capital: Dict[str, float] = {}
    journal_entry_ids: List[str] = []
    created_at: datetime = Field(default_factory=datetime.utcnow)


class Dissolution(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    dissolution_type: DissolutionType
    reason: DissolutionReason
    dissolution_date: datetime = Field(default_factory=datetime.utcnow)
    total_realization: float = 0
    total_liabilities: float = 0
    partner_capitals: Dict[str, float] = {}
    settlement: Dict[str, float] = {}
    loss_on_realization: float = 0
    journal_entry_ids: List[str] = []
    status: str = "pending"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class RevaluationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_name: str
    old_value: float
    new_value: float
    increase: float
    decrease: float
    journal_entry_id: Optional[str] = None


class PartnershipReport(BaseModel):
    partnership_id: str
    partnership_name: str
    report_date: datetime
    capital_accounts: List[PartnerCurrentAccount] = []
    partner_details: List[Partner] = []
    total_capital: float = 0
    total_profit_distributed: float = 0
    total_drawings: float = 0
    goodwill_amount: float = 0


# ============================================================================
# In-Memory Storage (Replace with Neo4j in Production)
# ============================================================================

partnerships: Dict[str, PartnershipAgreement] = {}
drawings: Dict[str, List[Drawing]] = {}
contributions: Dict[str, List[CapitalContribution]] = {}
profit_distributions: List[ProfitDistribution] = []
goodwill_revaluations: List[GoodwillRevaluation] = []
admissions: List[PartnerAdmission] = []
retirements: List[PartnerRetirement] = []
dissolutions: List[Dissolution] = {}
revaluation_entries: List[RevaluationEntry] = []


# ============================================================================
# Internal Service Communication
# ============================================================================


async def call_accounting_service(
    method: str, endpoint: str, data: Optional[Dict] = None
) -> Dict[str, Any]:
    """Call the main accounting service for journal entries and accounts."""
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "GET":
                response = await client.get(url)
            elif method == "POST":
                response = await client.post(url, json=data)
            elif method == "PUT":
                response = await client.put(url, json=data)
            elif method == "DELETE":
                response = await client.delete(url)
            else:
                raise ValueError(f"Unsupported HTTP method: {method}")

            if response.status_code in [200, 201]:
                return response.json() if response.text else {}
            else:
                logger.warning(
                    "accounting_service_call_failed",
                    method=method,
                    endpoint=endpoint,
                    status=response.status_code,
                )
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


async def check_feature_enabled(feature_name: str) -> bool:
    """Check if a feature is enabled via admin service."""
    try:
        async with httpx.AsyncClient(timeout=10.0) as client:
            response = await client.get(
                f"{ADMIN_SERVICE_URL}/features/{feature_name}"
            )
            if response.status_code == 200:
                return response.json().get("enabled", True)
    except Exception:
        pass
    return True


# ============================================================================
# Helper Functions
# ============================================================================


def calculate_interest_on_capital(
    capital: float, rate: float, period_months: int = 12
) -> float:
    """Calculate interest on partner's capital."""
    return capital * (rate / 100) * (period_months / 12)


def calculate_interest_on_drawings(
    amount: float, rate: float, months_outstanding: int
) -> float:
    """Calculate interest on partner's drawings (product method)."""
    return amount * (rate / 100) * (months_outstanding / 12)


def calculate_partner_current_account(
    partnership_id: str,
    partner_id: str,
    period_start: datetime,
    period_end: datetime,
) -> PartnerCurrentAccount:
    """Calculate a partner's current account for a period."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise ValueError(f"Partnership {partnership_id} not found")

    partner = next((p for p in partnership.partners if p.id == partner_id), None)
    if not partner:
        raise ValueError(f"Partner {partner_id} not found")

    # Get drawings for period
    partner_drawings = [
        d for d in drawings.get(partnership_id, [])
        if d.partner_id == partner_id
        and period_start <= d.date <= period_end
    ]
    total_drawings = sum(d.amount for d in partner_drawings)

    # Get contributions for period
    partner_contributions = [
        c for c in contributions.get(partnership_id, [])
        if c.partner_id == partner_id
        and period_start <= c.date <= period_end
    ]
    total_contributions = sum(c.amount for c in partner_contributions)

    # Calculate interest on drawings (assume 6 months average outstanding)
    interest_drawings = calculate_interest_on_drawings(total_drawings, 12, 6)

    # Calculate interest on capital
    interest_capital = calculate_interest_on_capital(
        partner.capital_contribution, partner.interest_rate_on_capital
    )

    # Net effect calculation
    net_effect = (
        total_contributions
        - total_drawings
        + interest_capital
        - interest_drawings
        + partner.guaranteed_salary
    )

    return PartnerCurrentAccount(
        partner_id=partner_id,
        partner_name=partner.name,
        partner_type=partner.partner_type,
        opening_balance=partner.capital_contribution,
        contributions=total_contributions,
        drawings=total_drawings,
        profit_share=0,  # Set during profit distribution
        guaranteed_salary=partner.guaranteed_salary,
        interest_on_capital=interest_capital,
        interest_on_drawings=interest_drawings,
        net_effect=net_effect,
        closing_balance=partner.capital_contribution + net_effect,
    )


def get_total_capital(partnership: PartnershipAgreement) -> float:
    """Get total capital of all partners."""
    return sum(p.capital_contribution for p in partnership.partners if p.is_active)


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
        "description": "Partnership-specific accounting including partner management, profit sharing, capital accounts, and dissolution",
    }


# ============================================================================
# API Endpoints - Partnership Management
# ============================================================================


@app.post(
    "/partnerships",
    response_model=PartnershipAgreement,
    status_code=status.HTTP_201_CREATED,
)
async def create_partnership(data: PartnershipAgreement):
    """Create a new partnership agreement."""
    if not await check_feature_enabled("partnership_accounting"):
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="Partnership accounting feature is not enabled",
        )

    partnership_id = str(uuid.uuid4())
    data.id = partnership_id
    data.created_at = datetime.utcnow()
    data.updated_at = datetime.utcnow()

    partnerships[partnership_id] = data
    drawings[partnership_id] = []
    contributions[partnership_id] = []

    # Create capital accounts in accounting service
    for partner in data.partners:
        await call_accounting_service(
            "POST",
            "/accounts",
            {
                "code": f"3000-{partner.name[:4].upper()}",
                "name": f"Partner Capital - {partner.name}",
                "account_type": "equity",
                "sub_type": "partner_capital",
                "description": f"Capital account for partner {partner.name}",
                "is_active": True,
            },
        )

    await call_audit_service(
        "CREATE",
        "partnership",
        partnership_id,
        {"name": data.name, "partner_count": len(data.partners)},
    )

    logger.info("partnership_created", partnership_id=partnership_id, name=data.name)
    return data


@app.get("/partnerships/{partnership_id}")
async def get_partnership(partnership_id: str):
    """Get partnership details."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )
    return partnership


@app.get("/partnerships")
async def list_partnerships(
    is_active: Optional[bool] = None,
    partnership_type: Optional[PartnershipType] = None,
):
    """List all partnerships with optional filters."""
    result = list(partnerships.values())

    if is_active is not None:
        result = [p for p in result if p.is_active == is_active]

    if partnership_type:
        result = [p for p in result if p.partnership_type == partnership_type]

    return {"partnerships": result, "count": len(result)}


@app.put("/partnerships/{partnership_id}")
async def update_partnership(partnership_id: str, data: Dict[str, Any]):
    """Update partnership details."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    # Update allowed fields
    for key, value in data.items():
        if hasattr(partnership, key) and key not in ["id", "created_at"]:
            setattr(partnership, key, value)

    partnership.updated_at = datetime.utcnow()
    partnerships[partnership_id] = partnership

    await call_audit_service(
        "UPDATE", "partnership", partnership_id, {"updated_fields": list(data.keys())}
    )

    return partnership


@app.delete("/partnerships/{partnership_id}")
async def delete_partnership(partnership_id: str):
    """Soft delete a partnership."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    partnership.is_active = False
    partnership.updated_at = datetime.utcnow()

    await call_audit_service("DELETE", "partnership", partnership_id, {})

    return {"message": "Partnership deactivated", "id": partnership_id}


# ============================================================================
# API Endpoints - Partner Management
# ============================================================================


@app.post(
    "/partnerships/{partnership_id}/partners",
    response_model=Partner,
    status_code=status.HTTP_201_CREATED,
)
async def add_partner(partnership_id: str, data: Partner):
    """Add a new partner to the partnership."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()
    data.updated_at = datetime.utcnow()

    partnership.partners.append(data)
    partnership.updated_at = datetime.utcnow()

    # Create capital account
    await call_accounting_service(
        "POST",
        "/accounts",
        {
            "code": f"3000-{data.name[:4].upper()}",
            "name": f"Partner Capital - {data.name}",
            "account_type": "equity",
            "sub_type": "partner_capital",
            "description": f"Capital account for partner {data.name}",
            "is_active": True,
        },
    )

    await call_audit_service(
        "CREATE", "partner", data.id, {"partnership_id": partnership_id, "name": data.name}
    )

    return data


@app.put("/partnerships/{partnership_id}/partners/{partner_id}")
async def update_partner(
    partnership_id: str, partner_id: str, data: Dict[str, Any]
):
    """Update partner details."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    partner = next((p for p in partnership.partners if p.id == partner_id), None)
    if not partner:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partner {partner_id} not found",
        )

    for key, value in data.items():
        if hasattr(partner, key) and key not in ["id", "created_at"]:
            setattr(partner, key, value)

    partner.updated_at = datetime.utcnow()

    await call_audit_service(
        "UPDATE", "partner", partner_id, {"updated_fields": list(data.keys())}
    )

    return partner


@app.post("/partnerships/{partnership_id}/partners/{partner_id}/retire")
async def retire_partner(
    partnership_id: str,
    partner_id: str,
    retirement_type: str = "normal",
    goodwill_amount: float = 0,
    payment_type: PaymentType = PaymentType.CASH,
):
    """Retire a partner from the partnership."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    partner = next((p for p in partnership.partners if p.id == partner_id), None)
    if not partner:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partner {partner_id} not found",
        )

    # Calculate amount payable (capital + goodwill share)
    amount_payable = partner.capital_contribution
    if goodwill_amount > 0:
        amount_payable += goodwill_amount * (partner.profit_sharing_ratio / 100)

    # Create journal entry for payment
    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Payment to retiring partner: {partner.name}",
        "entries": [
            {
                "account_code": f"3000-{partner.name[:4].upper()}",
                "description": f"Partner Capital - {partner.name}",
                "debit": amount_payable,
                "credit": 0,
            },
            {
                "account_code": "1000",  # Cash account
                "description": "Cash/Bank",
                "debit": 0,
                "credit": amount_payable,
            },
        ],
        "reference": f"RETIRE-{partner_id[:8]}",
    }

    journal_result = await call_accounting_service(
        "POST", "/journal-entries", journal_entry
    )

    # Update partner status
    partner.is_active = False
    partner.retirement_date = datetime.utcnow()
    partnership.updated_at = datetime.utcnow()

    # Record retirement
    retirement = PartnerRetirement(
        retiring_partner_id=partner_id,
        remaining_partners=[p.id for p in partnership.partners if p.id != partner_id],
        retirement_type=retirement_type,
        amount_payable=amount_payable,
        payment_type=payment_type,
        goodwill_amount=goodwill_amount,
        old_capital={partner.id: partner.capital_contribution},
        journal_entry_ids=[journal_result.get("id", "")] if journal_result else [],
    )
    retirements.append(retirement)

    await call_audit_service(
        "RETIRE", "partner", partner_id, {"amount_payable": amount_payable}
    )

    return {
        "retirement": retirement,
        "journal_entry_id": journal_result.get("id"),
        "message": f"Partner {partner.name} retired successfully",
    }


# ============================================================================
# API Endpoints - Drawings & Contributions
# ============================================================================


@app.post(
    "/partnerships/{partnership_id}/drawings",
    response_model=Drawing,
    status_code=status.HTTP_201_CREATED,
)
async def record_drawing(partnership_id: str, data: Drawing):
    """Record a partner's drawing."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    partner = next((p for p in partnership.partners if p.id == data.partner_id), None)
    if not partner:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partner {data.partner_id} not found",
        )

    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()

    if partnership_id not in drawings:
        drawings[partnership_id] = []
    drawings[partnership_id].append(data)

    # Create journal entry
    journal_entry = {
        "date": data.date,
        "description": f"Drawing by {partner.name}: {data.description}",
        "entries": [
            {
                "account_code": f"3100-{partner.name[:4].upper()}",
                "description": f"Drawing Account - {partner.name}",
                "debit": data.amount,
                "credit": 0,
            },
            {
                "account_code": "1000",
                "description": "Cash/Bank",
                "debit": 0,
                "credit": data.amount,
            },
        ],
        "reference": f"DRAW-{data.id[:8]}",
    }

    await call_accounting_service("POST", "/journal-entries", journal_entry)

    await call_audit_service(
        "CREATE", "drawing", data.id, {"partner_id": data.partner_id, "amount": data.amount}
    )

    return data


@app.get("/partnerships/{partnership_id}/drawings")
async def get_drawings(
    partnership_id: str,
    partner_id: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
):
    """Get drawings for a partnership."""
    partner_drawings = drawings.get(partnership_id, [])

    if partner_id:
        partner_drawings = [d for d in partner_drawings if d.partner_id == partner_id]

    if start_date:
        partner_drawings = [d for d in partner_drawings if d.date >= start_date]

    if end_date:
        partner_drawings = [d for d in partner_drawings if d.date <= end_date]

    total = sum(d.amount for d in partner_drawings)
    return {"drawings": partner_drawings, "total": total, "count": len(partner_drawings)}


@app.post(
    "/partnerships/{partnership_id}/contributions",
    response_model=CapitalContribution,
    status_code=status.HTTP_201_CREATED,
)
async def record_contribution(partnership_id: str, data: CapitalContribution):
    """Record a partner's capital contribution."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    partner = next((p for p in partnership.partners if p.id == data.partner_id), None)
    if not partner:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partner {data.partner_id} not found",
        )

    data.id = str(uuid.uuid4())
    data.created_at = datetime.utcnow()

    if partnership_id not in contributions:
        contributions[partnership_id] = []
    contributions[partnership_id].append(data)

    # Update partner's capital contribution
    partner.capital_contribution += data.amount
    partnership.updated_at = datetime.utcnow()

    # Create journal entry
    journal_entry = {
        "date": data.date,
        "description": f"Capital contribution by {partner.name}: {data.description}",
        "entries": [
            {
                "account_code": "1000",
                "description": "Cash/Bank",
                "debit": data.amount,
                "credit": 0,
            },
            {
                "account_code": f"3000-{partner.name[:4].upper()}",
                "description": f"Partner Capital - {partner.name}",
                "debit": 0,
                "credit": data.amount,
            },
        ],
        "reference": f"CONT-{data.id[:8]}",
    }

    await call_accounting_service("POST", "/journal-entries", journal_entry)

    await call_audit_service(
        "CREATE",
        "contribution",
        data.id,
        {"partner_id": data.partner_id, "amount": data.amount},
    )

    return data


@app.get("/partnerships/{partnership_id}/contributions")
async def get_contributions(
    partnership_id: str,
    partner_id: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
):
    """Get capital contributions for a partnership."""
    partner_contributions = contributions.get(partnership_id, [])

    if partner_id:
        partner_contributions = [
            c for c in partner_contributions if c.partner_id == partner_id
        ]

    if start_date:
        partner_contributions = [
            c for c in partner_contributions if c.date >= start_date
        ]

    if end_date:
        partner_contributions = [
            c for c in partner_contributions if c.date <= end_date
        ]

    total = sum(c.amount for c in partner_contributions)
    return {
        "contributions": partner_contributions,
        "total": total,
        "count": len(partner_contributions),
    }


# ============================================================================
# API Endpoints - Profit Distribution
# ============================================================================


@app.post(
    "/partnerships/{partnership_id}/profit-distribution",
    response_model=ProfitDistribution,
    status_code=status.HTTP_201_CREATED,
)
async def distribute_profit(partnership_id: str, total_profit: float):
    """Distribute profit among partners."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    if not partnership.partners:
        raise HTTPException(
            status_code=status.HTTP_400_BAD_REQUEST,
            detail="No partners in partnership",
        )

    distribution = ProfitDistribution(
        period_start=datetime.utcnow() - timedelta(days=365),
        period_end=datetime.utcnow(),
        total_profit=total_profit,
    )

    # Calculate guaranteed salaries first
    for partner in partnership.partners:
        if partner.guaranteed_salary > 0:
            distribution.guaranteed_salaries[partner.id] = partner.guaranteed_salary

    # Calculate interest on capital
    remaining_profit = total_profit
    for partner in partnership.partners:
        interest = calculate_interest_on_capital(
            partner.capital_contribution, partner.interest_rate_on_capital
        )
        if interest > 0:
            distribution.interest_on_capital[partner.id] = interest
            remaining_profit -= interest

    # Distribute remaining profit based on sharing ratio
    total_ratio = sum(
        p.profit_sharing_ratio for p in partnership.partners if p.is_active
    )
    if total_ratio > 0:
        for partner in partnership.partners:
            if partner.is_active:
                share = remaining_profit * (partner.profit_sharing_ratio / total_ratio)
                distribution.profit_shares[partner.id] = share
                distribution.final_distribution[partner.id] = (
                    distribution.guaranteed_salaries.get(partner.id, 0)
                    + distribution.interest_on_capital.get(partner.id, 0)
                    + share
                )

    # Create journal entry
    journal_entry = {
        "date": datetime.utcnow(),
        "description": f"Profit distribution for period ending {distribution.period_end.date()}",
        "entries": [],
        "reference": f"PROFIT-DIST-{distribution.id[:8]}",
    }

    # P&L appropriation
    journal_entry["entries"].append(
        {
            "account_code": "3900",
            "description": "Profit & Loss Appropriation",
            "debit": total_profit,
            "credit": 0,
        }
    )

    for partner_id, amount in distribution.final_distribution.items():
        partner = next((p for p in partnership.partners if p.id == partner_id), None)
        if partner and amount > 0:
            journal_entry["entries"].append(
                {
                    "account_code": f"3000-{partner.name[:4].upper()}",
                    "description": f"Partner Current Account - {partner.name}",
                    "debit": 0,
                    "credit": amount,
                }
            )

    journal_result = await call_accounting_service(
        "POST", "/journal-entries", journal_entry
    )
    distribution.journal_entry_id = journal_result.get("id")

    profit_distributions.append(distribution)

    await call_audit_service(
        "DISTRIBUTE", "profit", distribution.id, {"total_profit": total_profit}
    )

    return distribution


@app.get("/partnerships/{partnership_id}/profit-distributions")
async def get_profit_distributions(
    partnership_id: str,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
):
    """Get profit distribution history."""
    filtered = [
        d
        for d in profit_distributions
        if start_date is None or d.period_start >= start_date
    ]
    filtered = [
        d for d in filtered if end_date is None or d.period_end <= end_date
    ]

    return {"distributions": filtered, "count": len(filtered)}


# ============================================================================
# API Endpoints - Goodwill & Revaluation
# ============================================================================


@app.post(
    "/partnerships/{partnership_id}/revalue",
    response_model=GoodwillRevaluation,
    status_code=status.HTTP_201_CREATED,
)
async def revalue_partnership_assets(
    partnership_id: str,
    asset_revaluations: List[Dict[str, Any]],
    goodwill_amount: float = 0,
):
    """Revalue partnership assets and/or adjust goodwill."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    revaluation = GoodwillRevaluation(
        old_partners=[p.id for p in partnership.partners],
        goodwill_amount=goodwill_amount,
    )

    # Record asset revaluations
    journal_entries = []

    for rev in asset_revaluations:
        entry = RevaluationEntry(
            asset_name=rev["asset_name"],
            old_value=rev["old_value"],
            new_value=rev["new_value"],
            increase=rev.get("increase", 0),
            decrease=rev.get("decrease", 0),
        )
        revaluation_entries.append(entry)

        if entry.increase > 0:
            journal_entries.append(
                {
                    "account_code": rev["account_code"],
                    "description": f"Revaluation surplus - {entry.asset_name}",
                    "debit": entry.increase,
                    "credit": 0,
                }
            )
            journal_entries.append(
                {
                    "account_code": "3100",
                    "description": "Revaluation Reserve",
                    "debit": 0,
                    "credit": entry.increase,
                }
            )

        if entry.decrease > 0:
            journal_entries.append(
                {
                    "account_code": "4100",
                    "description": f"Revaluation Loss - {entry.asset_name}",
                    "debit": entry.decrease,
                    "credit": 0,
                }
            )
            journal_entries.append(
                {
                    "account_code": rev["account_code"],
                    "description": f"Asset reduction - {entry.asset_name}",
                    "debit": 0,
                    "credit": entry.decrease,
                }
            )

    # Goodwill treatment
    if goodwill_amount > 0:
        if partnership.goodwill_treatment == GoodwillTreatment.GOODWILL_RAISED:
            journal_entries.extend(
                [
                    {
                        "account_code": "1500",
                        "description": "Goodwill",
                        "debit": goodwill_amount,
                        "credit": 0,
                    },
                    {
                        "account_code": "3100",
                        "description": "Revaluation Reserve / Partners' Capital",
                        "debit": 0,
                        "credit": goodwill_amount,
                    },
                ]
            )
        elif (
            partnership.goodwill_treatment
            == GoodwillTreatment.GOODWILL_RAISED_AND_AMLORITZED
        ):
            journal_entries.extend(
                [
                    {
                        "account_code": "1500",
                        "description": "Goodwill",
                        "debit": goodwill_amount,
                        "credit": 0,
                    },
                    {
                        "account_code": "3100",
                        "description": "Revaluation Reserve / Partners' Capital",
                        "debit": 0,
                        "credit": goodwill_amount,
                    },
                    {
                        "account_code": "5100",
                        "description": "Goodwill Amortization",
                        "debit": goodwill_amount / 5,  # 5 years
                        "credit": 0,
                    },
                    {
                        "account_code": "1500",
                        "description": "Goodwill",
                        "debit": 0,
                        "credit": goodwill_amount / 5,
                    },
                ]
            )

    # Create combined journal entry
    if journal_entries:
        journal_entry = {
            "date": datetime.utcnow(),
            "description": f"Revaluation and goodwill adjustment",
            "entries": journal_entries,
            "reference": f"REV-{revaluation.id[:8]}",
        }
        result = await call_accounting_service(
            "POST", "/journal-entries", journal_entry
        )
        revaluation.revaluation_journal_entry_id = result.get("id")

    goodwill_revaluations.append(revaluation)

    await call_audit_service(
        "REVALUE",
        "partnership",
        partnership_id,
        {"goodwill_amount": goodwill_amount, "asset_count": len(asset_revaluations)},
    )

    return revaluation


# ============================================================================
# API Endpoints - Dissolution
# ============================================================================


@app.post(
    "/partnerships/{partnership_id}/dissolve",
    response_model=Dissolution,
    status_code=status.HTTP_201_CREATED,
)
async def dissolve_partnership(
    partnership_id: str,
    dissolution_type: DissolutionType,
    reason: DissolutionReason,
    asset_realizations: List[Dict[str, Any]],
    creditor_payments: List[Dict[str, Any]],
):
    """Dissolve the partnership."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    dissolution = Dissolution(
        dissolution_type=dissolution_type,
        reason=reason,
        dissolution_date=datetime.utcnow(),
    )

    # Calculate total realizations from asset sales
    total_realization = sum(a.get("sale_proceeds", 0) for a in asset_realizations)
    total_book_value = sum(a.get("book_value", 0) for a in asset_realizations)
    loss_on_realization = max(0, total_book_value - total_realization)
    dissolution.total_realization = total_realization
    dissolution.loss_on_realization = loss_on_realization

    # Calculate total liabilities paid
    total_liabilities = sum(c.get("amount", 0) for c in creditor_payments)
    dissolution.total_liabilities = total_liabilities

    # Partner capitals
    for partner in partnership.partners:
        dissolution.partner_capitals[partner.id] = partner.capital_contribution

    # Create journal entries for realizations
    journal_entries = []

    for asset in asset_realizations:
        # Record sale
        journal_entries.extend(
            [
                {
                    "account_code": "1000",
                    "description": f"Cash from sale: {asset['asset_name']}",
                    "debit": asset.get("sale_proceeds", 0),
                    "credit": 0,
                },
                {
                    "account_code": asset.get("account_code", "1200"),
                    "description": asset["asset_name"],
                    "debit": 0,
                    "credit": asset.get("book_value", 0),
                },
            ]
        )
        # Loss on realization
        if asset.get("sale_proceeds", 0) < asset.get("book_value", 0):
            loss = asset.get("book_value", 0) - asset.get("sale_proceeds", 0)
            journal_entries.extend(
                [
                    {
                        "account_code": "4200",
                        "description": f"Loss on realization: {asset['asset_name']}",
                        "debit": loss,
                        "credit": 0,
                    },
                    {
                        "account_code": asset.get("account_code", "1200"),
                        "description": asset["asset_name"],
                        "debit": 0,
                        "credit": loss,
                    },
                ]
            )

    # Pay creditors
    for creditor in creditor_payments:
        journal_entries.extend(
            [
                {
                    "account_code": creditor.get("account_code", "2000"),
                    "description": creditor.get("name", "Creditor"),
                    "debit": creditor.get("amount", 0),
                    "credit": 0,
                },
                {
                    "account_code": "1000",
                    "description": "Cash/Bank",
                    "debit": 0,
                    "credit": creditor.get("amount", 0),
                },
            ]
        )

    # Transfer loss to partners' capital accounts
    if loss_on_realization > 0:
        total_ratio = sum(
            p.profit_sharing_ratio for p in partnership.partners if p.is_active
        )
        for partner in partnership.partners:
            if partner.is_active and total_ratio > 0:
                partner_share = loss_on_realization * (
                    partner.profit_sharing_ratio / total_ratio
                )
                journal_entries.extend(
                    [
                        {
                            "account_code": f"3000-{partner.name[:4].upper()}",
                            "description": f"Loss on realization - {partner.name}",
                            "debit": partner_share,
                            "credit": 0,
                        },
                        {
                            "account_code": "4200",
                            "description": "Realization Account",
                            "debit": 0,
                            "credit": partner_share,
                        },
                    ]
                )

    # Partner final settlement
    remaining_cash = total_realization - total_liabilities
    for partner in partnership.partners:
        partner_share = remaining_cash * (
            partner.profit_sharing_ratio
            / sum(p.profit_sharing_ratio for p in partnership.partners if p.is_active)
            if partnership.partners
            else 0
        )
        dissolution.settlement[partner.id] = partner.capital_contribution + partner_share
        dissolution.partner_capitals[partner.id] = dissolution.settlement[partner.id]

        journal_entries.extend(
            [
                {
                    "account_code": f"3000-{partner.name[:4].upper()}",
                    "description": f"Final settlement - {partner.name}",
                    "debit": dissolution.settlement[partner.id],
                    "credit": 0,
                },
                {
                    "account_code": "1000",
                    "description": "Cash/Bank",
                    "debit": 0,
                    "credit": dissolution.settlement[partner.id],
                },
            ]
        )

    # Create journal entries
    if journal_entries:
        journal_entry = {
            "date": datetime.utcnow(),
            "description": f"Dissolution of partnership: {reason.value}",
            "entries": journal_entries,
            "reference": f"DISSOLVE-{dissolution.id[:8]}",
        }
        result = await call_accounting_service(
            "POST", "/journal-entries", journal_entry
        )
        dissolution.journal_entry_ids = [result.get("id", "")]

    dissolution.status = "completed"
    dissolutions[partnership_id] = dissolution
    partnership.is_active = False

    await call_audit_service(
        "DISSOLVE", "partnership", partnership_id, {"reason": reason.value}
    )

    return dissolution


# ============================================================================
# API Endpoints - Partner Reports
# ============================================================================


@app.get(
    "/partnerships/{partnership_id}/reports/capital-accounts",
    response_model=List[PartnerCurrentAccount],
)
async def get_capital_accounts_report(
    partnership_id: str,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
):
    """Get capital accounts report for all partners."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    if not start_date:
        start_date = datetime.utcnow() - timedelta(days=365)
    if not end_date:
        end_date = datetime.utcnow()

    accounts = []
    for partner in partnership.partners:
        account = calculate_partner_current_account(
            partnership_id, partner.id, start_date, end_date
        )
        accounts.append(account)

    return accounts


@app.get(
    "/partnerships/{partnership_id}/reports/partnership-summary",
    response_model=PartnershipReport,
)
async def get_partnership_summary_report(partnership_id: str):
    """Get comprehensive partnership summary report."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    # Get active partners' capital accounts
    capital_accounts = []
    for partner in partnership.partners:
        account = calculate_partner_current_account(
            partnership_id,
            partner.id,
            datetime.utcnow() - timedelta(days=365),
            datetime.utcnow(),
        )
        capital_accounts.append(account)

    # Calculate totals
    total_capital = sum(p.capital_contribution for p in partnership.partners)
    total_profit = sum(
        d.total_profit
        for d in profit_distributions
        if d.period_end >= datetime.utcnow() - timedelta(days=365)
    )
    total_drawings = sum(
        d.amount
        for d in drawings.get(partnership_id, [])
        if d.date >= datetime.utcnow() - timedelta(days=365)
    )

    return PartnershipReport(
        partnership_id=partnership_id,
        partnership_name=partnership.name,
        report_date=datetime.utcnow(),
        capital_accounts=capital_accounts,
        partner_details=partnership.partners,
        total_capital=total_capital,
        total_profit_distributed=total_profit,
        total_drawings=total_drawings,
        goodwill_amount=sum(g.goodwill_amount for g in goodwill_revaluations),
    )


@app.get("/partnerships/{partnership_id}/reports/partner/{partner_id}")
async def get_partner_report(
    partnership_id: str,
    partner_id: str,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
):
    """Get detailed report for a specific partner."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    partner = next((p for p in partnership.partners if p.id == partner_id), None)
    if not partner:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partner {partner_id} not found",
        )

    if not start_date:
        start_date = datetime.utcnow() - timedelta(days=365)
    if not end_date:
        end_date = datetime.utcnow()

    # Get capital account
    capital_account = calculate_partner_current_account(
        partnership_id, partner_id, start_date, end_date
    )

    # Get drawings
    partner_drawings = [
        d
        for d in drawings.get(partnership_id, [])
        if d.partner_id == partner_id and start_date <= d.date <= end_date
    ]

    # Get contributions
    partner_contributions = [
        c
        for c in contributions.get(partnership_id, [])
        if c.partner_id == partner_id and start_date <= c.date <= end_date
    ]

    return {
        "partner": partner,
        "capital_account": capital_account,
        "drawings": partner_drawings,
        "contributions": partner_contributions,
        "period": {"start": start_date, "end": end_date},
    }


# ============================================================================
# API Endpoints - Admission of New Partner
# ============================================================================


@app.post(
    "/partnerships/{partnership_id}/admit-partner",
    response_model=PartnerAdmission,
    status_code=status.HTTP_201_CREATED,
)
async def admit_new_partner(
    partnership_id: str,
    new_partner_data: Partner,
    admission_type: str = "new_admission",
    goodwill_paid_by_new_partner: float = 0,
):
    """Admit a new partner to the partnership."""
    partnership = partnerships.get(partnership_id)
    if not partnership:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Partnership {partnership_id} not found",
        )

    # Record old partner capitals
    old_partners = [
        {"id": p.id, "name": p.name, "capital": p.capital_contribution}
        for p in partnership.partners
    ]

    admission = PartnerAdmission(
        new_partner=new_partner_data,
        old_partners=old_partners,
        admission_type=admission_type,
        goodwill_paid_by_new_partner=goodwill_paid_by_new_partner,
    )

    # Add new partner
    new_partner_data.id = str(uuid.uuid4())
    new_partner_data.created_at = datetime.utcnow()
    new_partner_data.updated_at = datetime.utcnow()
    partnership.partners.append(new_partner_data)

    # Calculate new capital balances
    if goodwill_paid_by_new_partner > 0:
        # Goodwill paid by new partner goes to old partners
        total_old_capital = sum(p.capital_contribution for p in old_partners)
        for old_partner in old_partners:
            old_partner["new_capital"] = old_partner["capital"] + (
                goodwill_paid_by_new_partner
                * (old_partner["capital"] / total_old_capital)
                if total_old_capital > 0
                else 0
            )
        admission.old_partners_renunciation = goodwill_paid_by_new_partner

    # Update partnership
    partnership.updated_at = datetime.utcnow()

    # Create journal entry for goodwill
    if goodwill_paid_by_new_partner > 0:
        journal_entry = {
            "date": datetime.utcnow(),
            "description": f"Goodwill paid by new partner {new_partner_data.name}",
            "entries": [
                {
                    "account_code": "1000",
                    "description": "Cash/Bank",
                    "debit": goodwill_paid_by_new_partner,
                    "credit": 0,
                },
                {
                    "account_code": "1500",
                    "description": "Goodwill",
                    "debit": 0,
                    "credit": goodwill_paid_by_new_partner,
                },
            ],
            "reference": f"GW-{admission.id[:8]}",
        }
        result = await call_accounting_service(
            "POST", "/journal-entries", journal_entry
        )
        admission.journal_entry_ids = [result.get("id", "")]

    admissions.append(admission)

    await call_audit_service(
        "ADMIT", "partner", new_partner_data.id, {"partnership_id": partnership_id}
    )

    return admission


# ============================================================================
# Main Entry Point
# ============================================================================


if __name__ == "__main__":
    import uvicorn

    logger.info("starting_partnership_accounting_service", port=PORT)
    uvicorn.run(app, host="0.0.0.0", port=PORT)