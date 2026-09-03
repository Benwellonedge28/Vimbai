"""
Vimbai NPO Scale Service
Lifecycle, scaling and donor-grade reporting for non-profits, plus
size-banding for commercial (sole trader -> enterprise) and
partnership organizations (partner capital accounts, profit sharing) and
private limited companies (shareholders, directors, dividends, equity),
public limited companies (listed-entity compliance),
plus vendors/purchases/creditors for every organization type, PDF
receipts and a public investor view for shareholders.

One service for every size of non-profit:
  * small          - community trust, savings club charity arm
  * medium         - single-office NGO, church with programs
  * large          - multi-branch national organization
  * extra_large    - federation / international chapters

What it adds on top of the npo-service accounting core:
  * Organization profiles with automatic size-band classification
  * Branch / chapter hierarchy with consolidated roll-up reporting
  * Donor CRM: donors, donations with automatic receipting,
    pledges and recurring schedules with a due-run collector
  * Public receipt verification (donor trust, anti-fraud)
  * Scale-aware governance: expense approval limits and dual
    approval enforcement for large / extra-large organizations
  * Budgets by fund and program with budget-vs-actual reporting
  * Compliance calendar (filing deadlines, policies)
  * Donor-grade statements: activities by fund, functional
    expenses, financial position - all branch-consolidated.
"""

import os
import sqlite3
import time
import uuid
from contextlib import contextmanager
from typing import Dict, List, Optional

import structlog
from fastapi import Depends, FastAPI, Header, HTTPException, Response
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "npo-scale-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "9021"))
DB_PATH = os.getenv("NPO_SCALE_DB", os.path.join(os.path.dirname(__file__), "vimbai_npo_scale.db"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
)
log = structlog.get_logger()

app = FastAPI(
    title="Vimbai NPO Scale Service",
    version=SERVICE_VERSION,
    description="Non-profit lifecycle, scaling and donor reporting.",
)
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

# ---------------------------------------------------------------------------
# Size bands and scale-aware feature flags
# ---------------------------------------------------------------------------

SIZE_BANDS = ["small", "medium", "large", "extra_large"]

ORG_TYPES = ["nonprofit", "commercial", "partnership", "company", "plc"]

# Partnership revenue (USD) thresholds: small firm -> international LLP.
BAND_REVENUE_THRESHOLDS_PARTNERSHIP = [
    ("small", 500_000),
    ("medium", 5_000_000),
    ("large", 50_000_000),
    ("extra_large", float("inf")),
]

# Commercial revenue (USD) thresholds: sole trader -> enterprise.
BAND_REVENUE_THRESHOLDS_COMMERCIAL = [
    ("sole_trader", 50_000),
    ("small", 500_000),
    ("medium", 5_000_000),
    ("large", 50_000_000),
    ("extra_large", float("inf")),
]

# Annual revenue (USD) thresholds used for automatic classification.
BAND_REVENUE_THRESHOLDS = [
    ("small", 50_000),
    ("medium", 500_000),
    ("large", 5_000_000),
    ("extra_large", float("inf")),
]

FEATURES_BY_BAND: Dict[str, List[str]] = {
    "sole_trader": [
        "core_accounting",
        "cash_book",
        "self_service_setup",
        "sales_receipting",
        "tax_calendar",
    ],
    "small": [
        "core_accounting",
        "donor_crm",
        "automatic_receipting",
        "compliance_calendar",
    ],
    "medium": [
        "core_accounting",
        "donor_crm",
        "automatic_receipting",
        "compliance_calendar",
        "program_budgets",
        "recurring_donations",
        "impact_reporting",
    ],
    "large": [
        "core_accounting",
        "donor_crm",
        "automatic_receipting",
        "compliance_calendar",
        "program_budgets",
        "recurring_donations",
        "impact_reporting",
        "branch_hierarchy",
        "consolidated_reporting",
        "dual_approval_expenses",
    ],
    "extra_large": [
        "core_accounting",
        "donor_crm",
        "automatic_receipting",
        "compliance_calendar",
        "program_budgets",
        "recurring_donations",
        "impact_reporting",
        "branch_hierarchy",
        "consolidated_reporting",
        "dual_approval_expenses",
        "federation_chapters",
        "cross_border_consolidation",
    ],
}

# Expense approval policy per band: single-signer limit above which
# a second approver is required (dual approval).
APPROVAL_LIMITS: Dict[str, float] = {
    "sole_trader": float("inf"),
    "small": float("inf"),
    "medium": 5_000,
    "large": 2_000,
    "extra_large": 1_000,
}

# Feature ladders per organization type. Commercial organizations get
# the full business stack; non-profits keep their fund-accounting set.
FEATURES: Dict[str, Dict[str, List[str]]] = {
    "nonprofit": FEATURES_BY_BAND,
    # Private limited companies: share capital at every size, corporate
    # stack as they grow toward group holding structures.
    "company": {
        "small": [
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "cash_book",
            "sales_receipting",
            "tax_calendar",
        ],
        "medium": [
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
        ],
        "large": [
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "ifrs_reports",
            "audit_trail",
        ],
        "extra_large": [
            "share_capital",
            "shareholders_register",
            "statutory_registers",
            "dividends",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "ifrs_reports",
            "audit_trail",
            "subsidiaries",
            "group_consolidation",
            "intercompany",
        ],
    },
    # Partnerships: partner equity at every size, business stack as they grow
    "partnership": {
        "small": [
            "partner_capital_accounts",
            "profit_sharing",
            "partner_draws",
            "cash_book",
            "sales_receipting",
            "tax_calendar",
        ],
        "medium": [
            "partner_capital_accounts",
            "profit_sharing",
            "partner_draws",
            "cash_book",
            "sales_receipting",
            "tax_calendar",
            "inventory_lite",
            "payroll",
            "multi_currency",
        ],
        "large": [
            "partner_capital_accounts",
            "profit_sharing",
            "partner_draws",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "multi_entity",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "ifrs_reports",
        ],
        "extra_large": [
            "partner_capital_accounts",
            "profit_sharing",
            "partner_draws",
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "multi_entity",
            "branch_hierarchy",
            "consolidated_reporting",
            "dual_approval_expenses",
            "ifrs_reports",
            "intercompany",
            "group_consolidation",
            "joint_venture_accounts",
        ],
    },
    "commercial": {
        "sole_trader": FEATURES_BY_BAND["sole_trader"],
        "small": FEATURES_BY_BAND["small"] + ["sales_receipting", "inventory_lite"],
        "medium": FEATURES_BY_BAND["medium"] + ["sales_receipting", "inventory_lite", "payroll", "multi_currency"],
        "large": FEATURES_BY_BAND["large"]
        + ["sales_receipting", "inventory_lite", "payroll", "multi_currency", "multi_entity", "ifrs_reports"],
        "extra_large": FEATURES_BY_BAND["extra_large"]
        + [
            "sales_receipting",
            "inventory_lite",
            "payroll",
            "multi_currency",
            "multi_entity",
            "ifrs_reports",
            "intercompany",
            "group_consolidation",
        ],
    },
}

# Public limited companies: company stack + listed-entity compliance.
FEATURES["plc"] = {
    "small": FEATURES["company"]["small"] + ["public_share_registry"],
    "medium": FEATURES["company"]["medium"] + ["public_share_registry", "mandatory_audit"],
    "large": FEATURES["company"]["large"] + ["public_share_registry", "mandatory_audit", "public_disclosure"],
    "extra_large": FEATURES["company"]["extra_large"]
    + [
        "public_share_registry",
        "mandatory_audit",
        "public_disclosure",
        "listing_compliance",
        "sec_filings",
    ],
}


def classify_band(
    annual_revenue: float,
    headcount: int,
    branches: int,
    org_type: str = "nonprofit",
) -> str:
    """Automatic size-band classification.

    Revenue is the primary signal; headcount and branch count push an
    organization up a band so growth is never blocked by classification.
    """
    ladder = BAND_REVENUE_THRESHOLDS
    if org_type in ("partnership", "company", "plc"):
        # partnerships and private companies start at small
        ladder = BAND_REVENUE_THRESHOLDS_PARTNERSHIP
    elif org_type == "commercial":
        ladder = BAND_REVENUE_THRESHOLDS_COMMERCIAL
    band = ladder[0][0]
    for name, threshold in ladder:
        if annual_revenue < threshold:
            band = name
            break
    if branches > 25 or headcount > 500:
        band = "extra_large"
    elif branches > 5 or headcount > 50:
        if band not in ("large", "extra_large"):
            band = "large"
    return band


# ---------------------------------------------------------------------------
# Database
# ---------------------------------------------------------------------------

SCHEMA = """
CREATE TABLE IF NOT EXISTS orgs (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL,
    org_type TEXT NOT NULL DEFAULT 'nonprofit',
    sector TEXT DEFAULT 'community',
    country TEXT DEFAULT 'ZW',
    currency TEXT DEFAULT 'USD',
    size_band TEXT NOT NULL DEFAULT 'small',
    annual_revenue REAL DEFAULT 0,
    headcount INTEGER DEFAULT 0,
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS branches (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    parent_id TEXT,
    name TEXT NOT NULL,
    region TEXT DEFAULT '',
    status TEXT DEFAULT 'active',
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS donors (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    name TEXT NOT NULL,
    email TEXT DEFAULT '',
    phone TEXT DEFAULT '',
    type TEXT DEFAULT 'individual',
    is_recurring INTEGER DEFAULT 0,
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS vendors (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    name TEXT NOT NULL,
    phone TEXT DEFAULT '',
    email TEXT DEFAULT '',
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS purchases (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    vendor_id TEXT NOT NULL,
    description TEXT DEFAULT '',
    amount REAL NOT NULL,
    currency TEXT DEFAULT 'USD',
    status TEXT DEFAULT 'unpaid',
    created_at REAL NOT NULL,
    paid_at REAL
);
CREATE TABLE IF NOT EXISTS directors (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    name TEXT NOT NULL,
    appointed_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS shareholders (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    name TEXT NOT NULL,
    shares INTEGER DEFAULT 0,
    share_class TEXT DEFAULT 'ordinary',
    amount_paid REAL DEFAULT 0,
    verify_code TEXT,
    joined_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS dividends (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    per_share REAL NOT NULL,
    total REAL NOT NULL,
    declared_at REAL NOT NULL,
    status TEXT DEFAULT 'declared'
);
CREATE TABLE IF NOT EXISTS partners (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    name TEXT NOT NULL,
    capital_contribution REAL DEFAULT 0,
    profit_share REAL DEFAULT 0,
    status TEXT DEFAULT 'active',
    joined_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS partner_draws (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    partner_id TEXT NOT NULL,
    amount REAL NOT NULL,
    drawn_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS revenues (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    branch_id TEXT,
    source TEXT DEFAULT 'sale',
    customer TEXT DEFAULT '',
    amount REAL NOT NULL,
    currency TEXT DEFAULT 'USD',
    received_at REAL NOT NULL,
    receipt_id TEXT
);
CREATE TABLE IF NOT EXISTS donations (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    branch_id TEXT,
    donor_id TEXT,
    amount REAL NOT NULL,
    currency TEXT DEFAULT 'USD',
    designation TEXT DEFAULT 'general',
    source TEXT DEFAULT 'direct',
    received_at REAL NOT NULL,
    receipt_id TEXT
);
CREATE TABLE IF NOT EXISTS pledges (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    donor_id TEXT NOT NULL,
    amount REAL NOT NULL,
    currency TEXT DEFAULT 'USD',
    frequency TEXT DEFAULT 'one-time',
    next_due REAL,
    status TEXT DEFAULT 'active',
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS receipts (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    donation_id TEXT NOT NULL,
    donor_id TEXT,
    receipt_no TEXT UNIQUE NOT NULL,
    amount REAL NOT NULL,
    currency TEXT DEFAULT 'USD',
    token TEXT NOT NULL,
    issued_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS expenses (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    branch_id TEXT,
    fund TEXT DEFAULT 'general',
    program TEXT DEFAULT '',
    functional_area TEXT DEFAULT 'program',
    amount REAL NOT NULL,
    currency TEXT DEFAULT 'USD',
    description TEXT DEFAULT '',
    spent_at REAL NOT NULL,
    approver1 TEXT NOT NULL,
    approver2 TEXT,
    status TEXT DEFAULT 'approved'
);
CREATE TABLE IF NOT EXISTS budgets (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    fiscal_year INTEGER NOT NULL,
    fund TEXT NOT NULL,
    program TEXT DEFAULT '',
    budgeted REAL NOT NULL,
    UNIQUE (org_id, fiscal_year, fund, program)
);
CREATE TABLE IF NOT EXISTS compliance_items (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    title TEXT NOT NULL,
    category TEXT DEFAULT 'statutory',
    due_date REAL NOT NULL,
    responsible TEXT DEFAULT '',
    status TEXT DEFAULT 'open',
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS balance_items (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    amount REAL NOT NULL,
    currency TEXT DEFAULT 'USD',
    as_of REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_donations_org ON donations (org_id);
CREATE INDEX IF NOT EXISTS idx_expenses_org ON expenses (org_id);
"""

DB_PATH_LOCAL = os.getenv("NPO_SCALE_DB")
if DB_PATH_LOCAL:
    DB_PATH = DB_PATH_LOCAL


@contextmanager
def db():
    conn = sqlite3.connect(DB_PATH, timeout=15)
    conn.row_factory = sqlite3.Row
    conn.execute("PRAGMA foreign_keys = ON")
    try:
        yield conn
        conn.commit()
    finally:
        conn.close()


def init_db():
    with db() as conn:
        conn.executescript(SCHEMA)
        try:
            conn.execute("ALTER TABLE orgs ADD COLUMN org_type TEXT NOT NULL" " DEFAULT 'nonprofit'")
        except sqlite3.OperationalError:
            pass  # column already exists
        try:
            conn.execute("ALTER TABLE shareholders ADD COLUMN verify_code TEXT")
        except sqlite3.OperationalError:
            pass  # column already exists


init_db()


def current_user(x_user_id: Optional[str] = Header(default=None)) -> str:
    if not x_user_id:
        raise HTTPException(status_code=401, detail="X-User-ID header required")
    return x_user_id


def require_org(conn, org_id: str) -> sqlite3.Row:
    row = conn.execute("SELECT * FROM orgs WHERE id=?", (org_id,)).fetchone()
    if row is None:
        raise HTTPException(status_code=404, detail="Organization not found")
    return row


def refresh_band(conn, org_id: str) -> str:
    """Recompute the size band after membership/branch/revenue changes."""
    org = require_org(conn, org_id)
    n_branches = conn.execute("SELECT COUNT(*) c FROM branches WHERE org_id=?", (org_id,)).fetchone()["c"]
    band = classify_band(
        org["annual_revenue"] or 0,
        org["headcount"] or 0,
        n_branches,
        org["org_type"],
    )
    conn.execute(
        "UPDATE orgs SET size_band=?, updated_at=? WHERE id=?",
        (band, time.time(), org_id),
    )
    return band


# ---------------------------------------------------------------------------
# Models
# ---------------------------------------------------------------------------


class OrgCreate(BaseModel):
    name: str
    org_type: str = "nonprofit"
    sector: str = "community"
    country: str = "ZW"
    currency: str = "USD"
    annual_revenue: float = 0
    headcount: int = 0


class OrgUpdate(BaseModel):
    annual_revenue: Optional[float] = None
    headcount: Optional[int] = None
    sector: Optional[str] = None


class BranchCreate(BaseModel):
    name: str
    parent_id: Optional[str] = None
    region: str = ""


class DonorCreate(BaseModel):
    name: str
    email: str = ""
    phone: str = ""
    type: str = "individual"


class DonationCreate(BaseModel):
    donor_id: str
    amount: float
    currency: str = "USD"
    designation: str = "general"
    source: str = "direct"
    branch_id: Optional[str] = None


class PledgeCreate(BaseModel):
    donor_id: str
    amount: float
    currency: str = "USD"
    frequency: str = "monthly"


class ExpenseCreate(BaseModel):
    amount: float
    fund: str = "general"
    program: str = ""
    functional_area: str = "program"
    description: str = ""
    branch_id: Optional[str] = None
    approver1: str
    approver2: Optional[str] = None


class BudgetUpsert(BaseModel):
    fiscal_year: int
    fund: str
    program: str = ""
    budgeted: float


class ComplianceCreate(BaseModel):
    title: str
    category: str = "statutory"
    due_date: float
    responsible: str = ""


class BalanceItemCreate(BaseModel):
    kind: str
    name: str
    amount: float
    currency: str = "USD"
    as_of: float = Field(default_factory=lambda: time.time())


def row(r) -> Optional[dict]:
    return dict(r) if r is not None else None


def rows(rs) -> List[dict]:
    return [dict(r) for r in rs]


# ---------------------------------------------------------------------------
# Root
# ---------------------------------------------------------------------------


@app.get("/health")
def health():
    return {"service": SERVICE_NAME, "status": "healthy", "version": SERVICE_VERSION}


@app.get("/")
def root():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "endpoints": [
            "GET  /health",
            "POST /orgs",
            "GET  /orgs",
            "GET  /orgs/{id}",
            "PATCH /orgs/{id}",
            "GET  /orgs/{id}/features",
            "POST /orgs/{id}/branches",
            "GET  /orgs/{id}/branches",
            "POST /orgs/{id}/donors",
            "GET  /orgs/{id}/donors",
            "POST /orgs/{id}/vendors",
            "GET  /orgs/{id}/vendors",
            "POST /orgs/{id}/purchases",
            "POST /orgs/{id}/purchases/{pid}/pay",
            "GET  /orgs/{id}/reports/creditors",
            "GET  /orgs/{id}/receipts/{rid}/pdf",
            "GET  /public/holdings/{verify_code}",
            "POST /orgs/{id}/directors",
            "GET  /orgs/{id}/directors",
            "POST /orgs/{id}/shareholders",
            "GET  /orgs/{id}/shareholders",
            "POST /orgs/{id}/dividends",
            "GET  /orgs/{id}/reports/equity",
            "POST /orgs/{id}/partners",
            "GET  /orgs/{id}/partners",
            "POST /orgs/{id}/partners/{pid}/draws",
            "GET  /orgs/{id}/reports/capital-accounts",
            "POST /orgs/{id}/revenues",
            "GET  /orgs/{id}/revenues",
            "POST /orgs/{id}/donations",
            "GET  /orgs/{id}/donations",
            "POST /orgs/{id}/pledges",
            "POST /orgs/{id}/pledges/run",
            "GET  /orgs/{id}/receipts",
            "GET  /receipts/verify/{token}",
            "POST /orgs/{id}/expenses",
            "GET  /orgs/{id}/expenses",
            "PUT  /orgs/{id}/budgets",
            "POST /orgs/{id}/compliance",
            "GET  /orgs/{id}/compliance",
            "POST /orgs/{id}/balance-items",
            "GET  /orgs/{id}/reports/activities",
            "GET  /orgs/{id}/reports/position",
            "GET  /orgs/{id}/reports/functional-expenses",
            "GET  /orgs/{id}/reports/budget-vs-actual",
            "GET  /orgs/{id}/reports/consolidated",
        ],
    }


# ---------------------------------------------------------------------------
# Organizations
# ---------------------------------------------------------------------------


@app.post("/orgs")
def create_org(body: OrgCreate, user: str = Depends(current_user)):
    now = time.time()
    org_id = str(uuid.uuid4())
    if body.org_type not in ORG_TYPES:
        raise HTTPException(
            status_code=400,
            detail="org_type must be nonprofit, commercial, partnership, company or plc",
        )
    band = classify_band(body.annual_revenue, body.headcount, 0, body.org_type)
    with db() as conn:
        conn.execute(
            "INSERT INTO orgs (id, owner_id, name, org_type, sector, country,"
            " currency, size_band, annual_revenue, headcount, created_at,"
            " updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            (
                org_id,
                user,
                body.name,
                body.org_type,
                body.sector,
                body.country,
                body.currency,
                band,
                body.annual_revenue,
                body.headcount,
                now,
                now,
            ),
        )
    return {
        "service": SERVICE_NAME,
        "org": {
            "id": org_id,
            "name": body.name,
            "org_type": body.org_type,
            "size_band": band,
        },
    }


@app.get("/orgs")
def list_orgs(user: str = Depends(current_user)):
    with db() as conn:
        orgs = rows(
            conn.execute(
                "SELECT * FROM orgs WHERE owner_id=? ORDER BY created_at DESC",
                (user,),
            )
        )
    return {"service": SERVICE_NAME, "orgs": orgs}


@app.get("/orgs/{org_id}")
def get_org(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        org = require_org(conn, org_id)
        features = FEATURES[org["org_type"]][org["size_band"]]
    return {"service": SERVICE_NAME, "org": row(org), "features": features}


@app.patch("/orgs/{org_id}")
def update_org(org_id: str, body: OrgUpdate, user: str = Depends(current_user)):
    with db() as conn:
        org = require_org(conn, org_id)
        updates, vals = [], []
        for field in ("annual_revenue", "headcount", "sector"):
            v = getattr(body, field)
            if v is not None:
                updates.append(f"{field}=?")
                vals.append(v)
        if updates:
            vals.append(org_id)
            conn.execute(f"UPDATE orgs SET {', '.join(updates)} WHERE id=?", vals)
        band = refresh_band(conn, org_id)
    return {"service": SERVICE_NAME, "status": "updated", "size_band": band}


@app.get("/orgs/{org_id}/features")
def org_features(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        org = require_org(conn, org_id)
    limit = APPROVAL_LIMITS[org["size_band"]]
    return {
        "service": SERVICE_NAME,
        "org_type": org["org_type"],
        "size_band": org["size_band"],
        "features": FEATURES[org["org_type"]][org["size_band"]],
        # None means no limit: sole traders and small orgs approve alone
        "approval_limit": None if limit == float("inf") else limit,
    }


# ---------------------------------------------------------------------------
# Branches / chapters
# ---------------------------------------------------------------------------


@app.post("/orgs/{org_id}/branches")
def create_branch(org_id: str, body: BranchCreate, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        branch_id = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO branches (id, org_id, parent_id, name, region,"
            " status, created_at) VALUES (?,?,?,?,?, 'active', ?)",
            (branch_id, org_id, body.parent_id, body.name, body.region, time.time()),
        )
        band = refresh_band(conn, org_id)
    return {"service": SERVICE_NAME, "branch_id": branch_id, "size_band": band}


@app.get("/orgs/{org_id}/branches")
def list_branches(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        branch_rows = rows(conn.execute("SELECT * FROM branches WHERE org_id=? ORDER BY created_at", (org_id,)))
    return {"service": SERVICE_NAME, "branches": branch_rows}


# ---------------------------------------------------------------------------
# Revenue (commercial organizations)
# ---------------------------------------------------------------------------


class RevenueEntryCreate(BaseModel):
    amount: float
    source: str = "sale"
    customer: str = ""
    currency: str = "USD"
    branch_id: Optional[str] = None


@app.post("/orgs/{org_id}/revenues")
def record_revenue(org_id: str, body: RevenueEntryCreate, user: str = Depends(current_user)):
    """Record business revenue (a sale, service invoice or other income)
    with an automatic verifiable receipt - the commercial mirror of the
    donation flow."""
    now = time.time()
    revenue_id = str(uuid.uuid4())
    with db() as conn:
        org = require_org(conn, org_id)
        if org["org_type"] not in ("commercial", "partnership", "company", "plc"):
            raise HTTPException(
                status_code=400,
                detail="Revenue entries are for commercial/partnership orgs",
            )
        conn.execute(
            "INSERT INTO revenues (id, org_id, branch_id, source, customer,"
            " amount, currency, received_at, receipt_id)"
            " VALUES (?,?,?,?,?,?,?,?,?)",
            (revenue_id, org_id, body.branch_id, body.source, body.customer, body.amount, body.currency, now, None),
        )
        seq = conn.execute("SELECT COUNT(*) c FROM receipts WHERE org_id=?", (org_id,)).fetchone()["c"] + 1
        receipt_no = "RCP-%s-%05d" % (org_id[:8], seq)
        conn.execute(
            "INSERT INTO receipts (id, org_id, donation_id, donor_id,"
            " receipt_no, amount, currency, token, issued_at)"
            " VALUES (?,?,?,?,?,?,?,?,?)",
            (
                str(uuid.uuid4()),
                org_id,
                revenue_id,
                None,
                receipt_no,
                body.amount,
                body.currency,
                uuid.uuid4().hex,
                now,
            ),
        )
    return {
        "service": SERVICE_NAME,
        "revenue_id": revenue_id,
        "receipt_no": receipt_no,
    }


@app.get("/orgs/{org_id}/revenues")
def list_revenues(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        revenue_rows = rows(
            conn.execute(
                "SELECT * FROM revenues WHERE org_id=? ORDER BY received_at DESC",
                (org_id,),
            )
        )
    return {"service": SERVICE_NAME, "revenues": revenue_rows}


# ---------------------------------------------------------------------------
# Vendors, purchases and creditors (all org types)
# ---------------------------------------------------------------------------


class VendorCreate(BaseModel):
    name: str
    phone: str = ""
    email: str = ""


class PurchaseCreate(BaseModel):
    vendor_id: str
    description: str = ""
    amount: float
    currency: str = "USD"


@app.post("/orgs/{org_id}/vendors")
def add_vendor(org_id: str, body: VendorCreate, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        vid = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO vendors (id, org_id, name, phone, email, created_at)" " VALUES (?,?,?,?,?,?)",
            (vid, org_id, body.name, body.phone, body.email, time.time()),
        )
    return {"service": SERVICE_NAME, "vendor_id": vid}


@app.get("/orgs/{org_id}/vendors")
def list_vendors(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        v = rows(
            conn.execute(
                "SELECT * FROM vendors WHERE org_id=? ORDER BY created_at",
                (org_id,),
            )
        )
    return {"service": SERVICE_NAME, "vendors": v}


@app.post("/orgs/{org_id}/purchases")
def record_purchase(org_id: str, body: PurchaseCreate, user: str = Depends(current_user)):
    """Record a purchase from a vendor on credit (unpaid)."""
    if body.amount <= 0:
        raise HTTPException(status_code=400, detail="amount must be positive")
    with db() as conn:
        require_org(conn, org_id)
        vendor = conn.execute(
            "SELECT id FROM vendors WHERE id=? AND org_id=?",
            (body.vendor_id, org_id),
        ).fetchone()
        if vendor is None:
            raise HTTPException(status_code=404, detail="Unknown vendor")
        pid = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO purchases (id, org_id, vendor_id, description,"
            " amount, currency, status, created_at)"
            " VALUES (?,?,?,?,?,?, 'unpaid', ?)",
            (pid, org_id, body.vendor_id, body.description, body.amount, body.currency, time.time()),
        )
    return {"service": SERVICE_NAME, "purchase_id": pid, "status": "unpaid"}


@app.post("/orgs/{org_id}/purchases/{purchase_id}/pay")
def pay_purchase(org_id: str, purchase_id: str, user: str = Depends(current_user)):
    """Settle a purchase: marks it paid and books it as an expense so
    every report (activities, equity, capital accounts) stays honest."""
    with db() as conn:
        require_org(conn, org_id)
        p = conn.execute(
            "SELECT * FROM purchases WHERE id=? AND org_id=?",
            (purchase_id, org_id),
        ).fetchone()
        if p is None:
            raise HTTPException(status_code=404, detail="Unknown purchase")
        if p["status"] == "paid":
            raise HTTPException(status_code=400, detail="Already paid")
        now = time.time()
        conn.execute(
            "UPDATE purchases SET status='paid', paid_at=? WHERE id=?",
            (now, purchase_id),
        )
        conn.execute(
            "INSERT INTO expenses (id, org_id, branch_id, fund, program,"
            " functional_area, amount, currency, description, spent_at,"
            " approver1, approver2, status)"
            " VALUES (?,?,?,?,?,?,?,?,?,?,?,?,'approved')",
            (
                str(uuid.uuid4()),
                org_id,
                None,
                "general",
                "",
                "operations",
                p["amount"],
                p["currency"],
                p["description"],
                now,
                user,
                None,
            ),
        )
    return {"service": SERVICE_NAME, "status": "paid"}


@app.get("/orgs/{org_id}/reports/creditors")
def report_creditors(org_id: str, user: str = Depends(current_user)):
    """Aged payables: what the org owes each vendor right now."""
    with db() as conn:
        require_org(conn, org_id)
        c = conn.execute(
            "SELECT v.name vendor, SUM(p.amount) owed"
            " FROM purchases p JOIN vendors v ON v.id = p.vendor_id"
            " WHERE p.org_id=? AND p.status='unpaid'"
            " GROUP BY v.name ORDER BY owed DESC",
            (org_id,),
        ).fetchall()
        creditors = [{"vendor": r["vendor"], "owed": r["owed"]} for r in c]
        total = sum(r["owed"] for r in c)
    return {
        "service": SERVICE_NAME,
        "creditors": creditors,
        "total_owed": total,
    }


# ---------------------------------------------------------------------------
# Private limited companies: shareholders, dividends, equity
# ---------------------------------------------------------------------------


class ShareholderCreate(BaseModel):
    name: str
    shares: int
    share_class: str = "ordinary"
    amount_paid: float = 0


class DividendCreate(BaseModel):
    per_share: float


@app.post("/orgs/{org_id}/shareholders")
def add_shareholder(org_id: str, body: ShareholderCreate, user: str = Depends(current_user)):
    if body.shares < 1:
        raise HTTPException(status_code=400, detail="shares must be >= 1")
    with db() as conn:
        org = require_org(conn, org_id)
        if org["org_type"] not in ("company", "plc"):
            raise HTTPException(
                status_code=400,
                detail="Shareholders are for company/PLC orgs",
            )
        sid = str(uuid.uuid4())
        verify_code = uuid.uuid4().hex
        conn.execute(
            "INSERT INTO shareholders (id, org_id, name, shares, share_class,"
            " amount_paid, verify_code, joined_at) VALUES (?,?,?,?,?,?,?,?)",
            (sid, org_id, body.name, body.shares, body.share_class, body.amount_paid, verify_code, time.time()),
        )
    return {"service": SERVICE_NAME, "shareholder_id": sid, "verify_code": verify_code}


class DirectorCreate(BaseModel):
    name: str


@app.post("/orgs/{org_id}/directors")
def add_director(org_id: str, body: DirectorCreate, user: str = Depends(current_user)):
    with db() as conn:
        org = require_org(conn, org_id)
        if org["org_type"] not in ("company", "plc"):
            raise HTTPException(
                status_code=400,
                detail="Directors are for company/PLC orgs",
            )
        did = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO directors (id, org_id, name, appointed_at)" " VALUES (?,?,?,?)",
            (did, org_id, body.name, time.time()),
        )
    return {"service": SERVICE_NAME, "director_id": did}


@app.get("/orgs/{org_id}/directors")
def list_directors(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        d = rows(
            conn.execute(
                "SELECT * FROM directors WHERE org_id=? ORDER BY appointed_at",
                (org_id,),
            )
        )
    return {"service": SERVICE_NAME, "directors": d}


@app.get("/orgs/{org_id}/shareholders")
def list_shareholders(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        sh_rows = rows(
            conn.execute(
                "SELECT * FROM shareholders WHERE org_id=? ORDER BY joined_at",
                (org_id,),
            )
        )
    return {"service": SERVICE_NAME, "shareholders": sh_rows}


@app.post("/orgs/{org_id}/dividends")
def declare_dividend(org_id: str, body: DividendCreate, user: str = Depends(current_user)):
    """Declare a dividend per share, capped at distributable reserves."""
    with db() as conn:
        org = require_org(conn, org_id)
        if org["org_type"] not in ("company", "plc"):
            raise HTTPException(
                status_code=400,
                detail="Dividends are for company/PLC orgs",
            )
        shareholders = conn.execute("SELECT shares FROM shareholders WHERE org_id=?", (org_id,)).fetchall()
        total_shares = sum(s["shares"] for s in shareholders)
        if total_shares == 0:
            raise HTTPException(status_code=400, detail="Add shareholders before declaring dividends")
        income = (
            conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM revenues WHERE org_id=?",
                (org_id,),
            ).fetchone()["t"]
            - conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM expenses WHERE org_id=?",
                (org_id,),
            ).fetchone()["t"]
            - conn.execute(
                "SELECT COALESCE(SUM(total),0) t FROM dividends WHERE org_id=?",
                (org_id,),
            ).fetchone()["t"]
        )
        if body.per_share <= 0:
            raise HTTPException(status_code=400, detail="per_share must be positive")
        total = body.per_share * total_shares
        if total > income:
            raise HTTPException(
                status_code=400,
                detail="Dividend exceeds distributable reserves (%.2f)" % income,
            )
        did = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO dividends (id, org_id, per_share, total, declared_at)" " VALUES (?,?,?,?,?)",
            (did, org_id, body.per_share, total, time.time()),
        )
    return {
        "service": SERVICE_NAME,
        "dividend_id": did,
        "total_shares": total_shares,
        "total": total,
    }


@app.get("/orgs/{org_id}/reports/equity")
def report_equity(org_id: str, user: str = Depends(current_user)):
    """Statement of changes in equity: share capital paid in,
    retained earnings, dividends declared, closing equity."""
    with db() as conn:
        org = require_org(conn, org_id)
        if org["org_type"] not in ("company", "plc"):
            raise HTTPException(
                status_code=400,
                detail="Equity statement is for company/PLC orgs",
            )
        capital = conn.execute(
            "SELECT COALESCE(SUM(amount_paid),0) t FROM shareholders" " WHERE org_id=?",
            (org_id,),
        ).fetchone()["t"]
        income = (
            conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM revenues WHERE org_id=?",
                (org_id,),
            ).fetchone()["t"]
            - conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM expenses WHERE org_id=?",
                (org_id,),
            ).fetchone()["t"]
        )
        divs = conn.execute(
            "SELECT COALESCE(SUM(total),0) t FROM dividends WHERE org_id=?",
            (org_id,),
        ).fetchone()["t"]
        shareholders = rows(conn.execute("SELECT * FROM shareholders WHERE org_id=?", (org_id,)))
        per_holder = [{"shareholder": s["name"], "shares": s["shares"]} for s in shareholders]
    retained = income - divs
    return {
        "service": SERVICE_NAME,
        "share_capital": capital,
        "retained_earnings": retained,
        "dividends_declared": divs,
        "total_equity": capital + retained,
    }


# ---------------------------------------------------------------------------
# Partnership equity: partners, draws, capital accounts
# ---------------------------------------------------------------------------


class PartnerCreate(BaseModel):
    name: str
    capital_contribution: float = 0
    profit_share: float = 0


class PartnerDrawCreate(BaseModel):
    amount: float


@app.post("/orgs/{org_id}/partners")
def add_partner(org_id: str, body: PartnerCreate, user: str = Depends(current_user)):
    with db() as conn:
        org = require_org(conn, org_id)
        if org["org_type"] != "partnership":
            raise HTTPException(
                status_code=400,
                detail="Partners can only be added to partnership orgs",
            )
        if not 0 <= body.profit_share <= 100:
            raise HTTPException(status_code=400, detail="profit_share must be 0-100")
        partner_id = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO partners (id, org_id, name, capital_contribution,"
            " profit_share, status, joined_at) VALUES (?,?,?,?,?, 'active',?)",
            (partner_id, org_id, body.name, body.capital_contribution, body.profit_share, time.time()),
        )
    return {"service": SERVICE_NAME, "partner_id": partner_id}


@app.get("/orgs/{org_id}/partners")
def list_partners(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        partner_rows = rows(
            conn.execute(
                "SELECT * FROM partners WHERE org_id=? ORDER BY joined_at",
                (org_id,),
            )
        )
    return {"service": SERVICE_NAME, "partners": partner_rows}


@app.post("/orgs/{org_id}/partners/{partner_id}/draws")
def partner_draw(org_id: str, partner_id: str, body: PartnerDrawCreate, user: str = Depends(current_user)):
    """A partner withdraws from their capital/profit share."""
    if body.amount <= 0:
        raise HTTPException(status_code=400, detail="amount must be positive")
    with db() as conn:
        require_org(conn, org_id)
        p = conn.execute(
            "SELECT * FROM partners WHERE id=? AND org_id=?",
            (partner_id, org_id),
        ).fetchone()
        if p is None:
            raise HTTPException(status_code=404, detail="Unknown partner")
        conn.execute(
            "INSERT INTO partner_draws (id, org_id, partner_id, amount," " drawn_at) VALUES (?,?,?,?,?)",
            (str(uuid.uuid4()), org_id, partner_id, body.amount, time.time()),
        )
    return {"service": SERVICE_NAME, "status": "drawn"}


@app.get("/orgs/{org_id}/reports/capital-accounts")
def report_capital_accounts(org_id: str, user: str = Depends(current_user)):
    """Statement of partners' capital accounts: contribution + share of
    net income - draws, per partner. The report partners care about."""
    with db() as conn:
        org = require_org(conn, org_id)
        if org["org_type"] != "partnership":
            raise HTTPException(
                status_code=400,
                detail="Capital accounts are for partnership orgs",
            )
        partners = rows(
            conn.execute(
                "SELECT * FROM partners WHERE org_id=? AND status='active'",
                (org_id,),
            )
        )
        income = (
            conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM revenues WHERE org_id=?",
                (org_id,),
            ).fetchone()["t"]
            + conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM donations WHERE org_id=?",
                (org_id,),
            ).fetchone()["t"]
            - conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM expenses WHERE org_id=?",
                (org_id,),
            ).fetchone()["t"]
        )
        total_share = sum(p["profit_share"] for p in partners)
        # shares not summing to 100 are allocated proportionally
        norm = total_share if total_share > 0 else 1.0
        accounts = []
        for p in partners:
            draws = conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM partner_draws" " WHERE partner_id=?",
                (p["id"],),
            ).fetchone()["t"]
            alloc = income * (p["profit_share"] / norm)
            accounts.append(
                {
                    "partner": p["name"],
                    "contribution": p["capital_contribution"],
                    "profit_share_pct": p["profit_share"],
                    "allocated_income": round(alloc, 2),
                    "draws": draws,
                    "capital_account": p["capital_contribution"] + alloc - draws,
                }
            )
    return {
        "service": SERVICE_NAME,
        "net_income": income,
        "accounts": accounts,
    }


# ---------------------------------------------------------------------------
# Donor CRM
# ---------------------------------------------------------------------------


@app.post("/orgs/{org_id}/donors")
def create_donor(org_id: str, body: DonorCreate, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        donor_id = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO donors (id, org_id, name, email, phone, type," " created_at) VALUES (?,?,?,?,?,?,?)",
            (donor_id, org_id, body.name, body.email, body.phone, body.type, time.time()),
        )
    return {"service": SERVICE_NAME, "donor_id": donor_id}


@app.get("/orgs/{org_id}/donors")
def list_donors(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        donor_rows = rows(conn.execute("SELECT * FROM donors WHERE org_id=? ORDER BY created_at", (org_id,)))
    return {"service": SERVICE_NAME, "donors": donor_rows}


@app.post("/orgs/{org_id}/donations")
def record_donation(org_id: str, body: DonationCreate, user: str = Depends(current_user)):
    """Record a donation and automatically issue a verifiable receipt."""
    now = time.time()
    donation_id = str(uuid.uuid4())
    with db() as conn:
        require_org(conn, org_id)
        donor = conn.execute(
            "SELECT * FROM donors WHERE id=? AND org_id=?",
            (body.donor_id, org_id),
        ).fetchone()
        if donor is None:
            raise HTTPException(status_code=404, detail="Unknown donor for this org")
        conn.execute(
            "INSERT INTO donations (id, org_id, branch_id, donor_id, amount,"
            " currency, designation, source, received_at, receipt_id)"
            " VALUES (?,?,?,?,?,?,?,?,?,?)",
            (
                donation_id,
                org_id,
                body.branch_id,
                body.donor_id,
                body.amount,
                body.currency,
                body.designation,
                body.source,
                now,
                None,
            ),
        )
        seq = conn.execute("SELECT COUNT(*) c FROM receipts WHERE org_id=?", (org_id,)).fetchone()["c"] + 1
        receipt_no = "RCP-%s-%05d" % (org_id[:8], seq)
        receipt_id = str(uuid.uuid4())
        token = uuid.uuid4().hex
        conn.execute(
            "INSERT INTO receipts (id, org_id, donation_id, donor_id,"
            " receipt_no, amount, currency, token, issued_at)"
            " VALUES (?,?,?,?,?,?,?,?,?)",
            (receipt_id, org_id, donation_id, body.donor_id, receipt_no, body.amount, body.currency, token, now),
        )
        conn.execute("UPDATE donations SET receipt_id=? WHERE id=?", (receipt_id, donation_id))
    return {
        "service": SERVICE_NAME,
        "donation_id": donation_id,
        "receipt": {
            "receipt_no": receipt_no,
            "token": token,
            "amount": body.amount,
            "currency": body.currency,
        },
    }


@app.get("/orgs/{org_id}/donations")
def list_donations(org_id: str, user: str = Depends(current_user), designation: Optional[str] = None):
    with db() as conn:
        require_org(conn, org_id)
        q = "SELECT * FROM donations WHERE org_id=?"
        args = [org_id]
        if designation:
            q += " AND designation=?"
            args.append(designation)
        donation_rows = rows(conn.execute(q + " ORDER BY received_at DESC", args))
    return {"service": SERVICE_NAME, "donations": donation_rows}


@app.post("/orgs/{org_id}/pledges")
def create_pledge(org_id: str, body: PledgeCreate, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        pledge_id = str(uuid.uuid4())
        first_due = time.time() + 30 * 86400
        conn.execute(
            "INSERT INTO pledges (id, org_id, donor_id, amount, currency,"
            " frequency, next_due, status, created_at)"
            " VALUES (?,?,?,?,?,?,?,?,?)",
            (
                pledge_id,
                org_id,
                body.donor_id,
                body.amount,
                body.currency,
                body.frequency,
                first_due,
                "active",
                time.time(),
            ),
        )
    return {"service": SERVICE_NAME, "pledge_id": pledge_id, "next_due": first_due}


@app.post("/orgs/{org_id}/pledges/run")
def run_pledges(org_id: str, user: str = Depends(current_user)):
    """Collect all recurring pledges that are due; turns each into a
    donation with a receipt (the scheduled-giving collector)."""
    now = time.time()
    collected = 0
    total = 0.0
    with db() as conn:
        require_org(conn, org_id)
        due = conn.execute(
            "SELECT * FROM pledges WHERE org_id=? AND status='active'" " AND next_due <= ?",
            (org_id, now),
        ).fetchall()
        for p in due:
            donation_id = str(uuid.uuid4())
            conn.execute(
                "INSERT INTO donations (id, org_id, branch_id, donor_id,"
                " amount, currency, designation, source, received_at,"
                " receipt_id) VALUES (?,?,NULL,?,?,?,?,?,?,NULL)",
                (donation_id, org_id, p["donor_id"], p["amount"], p["currency"], "pledge", "recurring", now),
            )
            seq = conn.execute("SELECT COUNT(*) c FROM receipts WHERE org_id=?", (org_id,)).fetchone()["c"] + 1
            receipt_no = "RCP-%s-%05d" % (org_id[:8], seq)
            conn.execute(
                "INSERT INTO receipts (id, org_id, donation_id, donor_id,"
                " receipt_no, amount, currency, token, issued_at)"
                " VALUES (?,?,?,?,?,?,?,?,?)",
                (
                    str(uuid.uuid4()),
                    org_id,
                    donation_id,
                    p["donor_id"],
                    receipt_no,
                    p["amount"],
                    p["currency"],
                    uuid.uuid4().hex,
                    now,
                ),
            )
            conn.execute(
                "UPDATE pledges SET next_due=? WHERE id=?",
                (now + 30 * 86400, p["id"]),
            )
            collected += 1
            total += p["amount"]
    return {
        "service": SERVICE_NAME,
        "collected": collected,
        "total": total,
    }


@app.get("/orgs/{org_id}/receipts")
def list_receipts(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        receipt_rows = rows(
            conn.execute(
                "SELECT * FROM receipts WHERE org_id=? ORDER BY issued_at DESC",
                (org_id,),
            )
        )
    return {"service": SERVICE_NAME, "receipts": receipt_rows}


@app.get("/receipts/verify/{token}")
def verify_receipt(token: str):
    """Public receipt verification - donors and auditors can confirm a
    receipt was genuinely issued by the organization (anti-fraud)."""
    with db() as conn:
        r = conn.execute(
            "SELECT r.receipt_no, r.amount, r.currency, r.issued_at, o.name"
            " FROM receipts r JOIN orgs o ON o.id = r.org_id WHERE r.token=?",
            (token,),
        ).fetchone()
    if r is None:
        raise HTTPException(status_code=404, detail="Invalid receipt token")
    return {"service": SERVICE_NAME, "valid": True, "receipt": row(r)}


def _pdf_escape(text: str) -> str:
    """Make a string safe for a PDF literal."""
    return text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)").encode("ascii", "replace").decode("ascii")


def _receipt_pdf(org_name: str, receipt) -> bytes:
    """Build a real, dependency-free PDF for a receipt."""
    issued = time.strftime("%Y-%m-%d", time.localtime(receipt["issued_at"]))
    lines = [
        (20, "OFFICIAL RECEIPT"),
        (14, org_name),
        (12, ""),
        (12, "Receipt No: %s" % receipt["receipt_no"]),
        (12, "Amount: %s %.2f" % (receipt["currency"], receipt["amount"])),
        (12, "Date issued: %s" % issued),
        (12, ""),
        (12, "Verify this receipt at:"),
        (10, "/receipts/verify/%s" % receipt["token"]),
        (12, ""),
        (10, "This receipt was issued by %s." % org_name),
    ]
    content = ["BT", "50 770 Td", "14 TL"]
    for size, text in lines:
        content.append("/F1 %d Tf" % size)
        content.append("(%s) Tj" % _pdf_escape(text))
        content.append("T*")
    content.append("ET")
    stream = "\n".join(content).encode("ascii")

    objs = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842]"
        b" /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>",
        b"<< /Length %d >>\nstream\n%s\nendstream" % (len(stream), stream),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for i, body in enumerate(objs, start=1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % i + body + b"\nendobj\n"
    xref_at = len(out)
    out += b"xref\n0 %d\n" % (len(objs) + 1)
    out += b"0000000000 65535 f \n"
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF" % (len(objs) + 1, xref_at)
    return bytes(out)


@app.get("/orgs/{org_id}/receipts/{receipt_id}/pdf")
def receipt_pdf(org_id: str, receipt_id: str, user: str = Depends(current_user)):
    """Download a receipt as a real PDF the donor can keep or print."""
    with db() as conn:
        require_org(conn, org_id)
        r = conn.execute(
            "SELECT r.receipt_no, r.amount, r.currency, r.issued_at, r.token"
            " FROM receipts r WHERE r.id=? AND r.org_id=?",
            (receipt_id, org_id),
        ).fetchone()
        if r is None:
            raise HTTPException(status_code=404, detail="Unknown receipt")
        org_name = conn.execute("SELECT name FROM orgs WHERE id=?", (org_id,)).fetchone()["name"]
    pdf = _receipt_pdf(org_name, r)
    return Response(
        content=pdf,
        media_type="application/pdf",
        headers={"Content-Disposition": 'attachment; filename="%s.pdf"' % r["receipt_no"]},
    )


@app.get("/public/holdings/{verify_code}")
def public_holdings(verify_code: str):
    """Public investor view: a shareholder verifies their own holding -
    shares, share class, capital and every dividend due - without seeing
    the company's books. Same trust model as receipt verification."""
    with db() as conn:
        s = conn.execute(
            "SELECT s.*, o.name org_name FROM shareholders s" " JOIN orgs o ON o.id = s.org_id WHERE s.verify_code=?",
            (verify_code,),
        ).fetchone()
        if s is None:
            raise HTTPException(status_code=404, detail="Invalid holding code")
        total_shares = conn.execute(
            "SELECT COALESCE(SUM(shares),0) t FROM shareholders" " WHERE org_id=?",
            (s["org_id"],),
        ).fetchone()["t"]
        div_rows = conn.execute(
            "SELECT per_share, total, declared_at FROM dividends" " WHERE org_id=? ORDER BY declared_at",
            (s["org_id"],),
        ).fetchall()
        dividends = [
            {
                "per_share": d["per_share"],
                "amount": d["per_share"] * s["shares"],
                "declared_at": d["declared_at"],
            }
            for d in div_rows
        ]
    pct = (s["shares"] / total_shares * 100) if total_shares else 0.0
    return {
        "service": SERVICE_NAME,
        "valid": True,
        "org_name": s["org_name"],
        "shareholder": s["name"],
        "shares": s["shares"],
        "share_class": s["share_class"],
        "capital_paid": s["amount_paid"],
        "percentage": round(pct, 2),
        "total_dividends": sum(d["amount"] for d in dividends),
        "dividends": dividends,
    }


# ---------------------------------------------------------------------------
# Expenses with scale-aware approval
# ---------------------------------------------------------------------------


@app.post("/orgs/{org_id}/expenses")
def create_expense(org_id: str, body: ExpenseCreate, user: str = Depends(current_user)):
    with db() as conn:
        org = require_org(conn, org_id)
        limit = APPROVAL_LIMITS[org["size_band"]]
        status = "approved"
        if body.amount > limit:
            if not body.approver2:
                status = "pending_dual_approval"
            if body.approver2 and body.approver2 == body.approver1:
                raise HTTPException(
                    status_code=400,
                    detail="Dual approval requires two different approvers",
                )
        expense_id = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO expenses (id, org_id, branch_id, fund, program,"
            " functional_area, amount, currency, description, spent_at,"
            " approver1, approver2, status) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (
                expense_id,
                org_id,
                body.branch_id,
                body.fund,
                body.program,
                body.functional_area,
                body.amount,
                "USD",
                body.description,
                time.time(),
                body.approver1,
                body.approver2,
                status,
            ),
        )
    return {"service": SERVICE_NAME, "expense_id": expense_id, "status": status}


@app.get("/orgs/{org_id}/expenses")
def list_expenses(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        expense_rows = rows(
            conn.execute(
                "SELECT * FROM expenses WHERE org_id=? ORDER BY spent_at DESC",
                (org_id,),
            )
        )
    return {"service": SERVICE_NAME, "expenses": expense_rows}


# ---------------------------------------------------------------------------
# Budgets, compliance, balance items
# ---------------------------------------------------------------------------


@app.put("/orgs/{org_id}/budgets")
def upsert_budget(org_id: str, body: BudgetUpsert, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        conn.execute(
            "INSERT INTO budgets (id, org_id, fiscal_year, fund, program,"
            " budgeted) VALUES (?,?,?,?,?,?)"
            " ON CONFLICT (org_id, fiscal_year, fund, program)"
            " DO UPDATE SET budgeted=excluded.budgeted",
            (str(uuid.uuid4()), org_id, body.fiscal_year, body.fund, body.program, body.budgeted),
        )
    return {"service": SERVICE_NAME, "status": "saved"}


@app.post("/orgs/{org_id}/compliance")
def create_compliance(org_id: str, body: ComplianceCreate, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        item_id = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO compliance_items (id, org_id, title, category,"
            " due_date, responsible, status, created_at)"
            " VALUES (?,?,?,?,?,?, 'open', ?)",
            (item_id, org_id, body.title, body.category, body.due_date, body.responsible, time.time()),
        )
    return {"service": SERVICE_NAME, "item_id": item_id}


@app.get("/orgs/{org_id}/compliance")
def list_compliance(org_id: str, user: str = Depends(current_user)):
    with db() as conn:
        require_org(conn, org_id)
        items = rows(
            conn.execute(
                "SELECT * FROM compliance_items WHERE org_id=?" " ORDER BY due_date ASC",
                (org_id,),
            )
        )
    return {"service": SERVICE_NAME, "compliance": items}


@app.post("/orgs/{org_id}/balance-items")
def create_balance_item(org_id: str, body: BalanceItemCreate, user: str = Depends(current_user)):
    if body.kind not in ("asset", "liability"):
        raise HTTPException(status_code=400, detail="kind must be asset or liability")
    with db() as conn:
        require_org(conn, org_id)
        item_id = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO balance_items (id, org_id, kind, name, amount," " currency, as_of) VALUES (?,?,?,?,?,?,?)",
            (item_id, org_id, body.kind, body.name, body.amount, body.currency, body.as_of),
        )
    return {"service": SERVICE_NAME, "item_id": item_id}


# ---------------------------------------------------------------------------
# Donor-grade reports
# ---------------------------------------------------------------------------


def _fy_where(fiscal_year: Optional[int]):
    if fiscal_year is None:
        return "", []
    start = time.mktime((fiscal_year, 1, 1, 0, 0, 0, 0, 0, 0))
    end = time.mktime((fiscal_year + 1, 1, 1, 0, 0, 0, 0, 0, 0))
    return " AND received_at >= ? AND received_at < ?", [start, end]


@app.get("/orgs/{org_id}/reports/activities")
def report_activities(org_id: str, user: str = Depends(current_user), fiscal_year: Optional[int] = None):
    """Statement of activities: revenue by fund (designation) minus
    expenses by fund, per branch with org total."""
    with db() as conn:
        org = require_org(conn, org_id)
        w, args = _fy_where(fiscal_year)
        if org["org_type"] in ("commercial", "partnership", "company", "plc"):
            revenue = conn.execute(
                "SELECT source fund, SUM(amount) total FROM revenues" " WHERE org_id=?" + w + " GROUP BY source",
                [org_id] + args,
            ).fetchall()
        else:
            revenue = conn.execute(
                "SELECT designation fund, SUM(amount) total FROM donations"
                " WHERE org_id=?" + w + " GROUP BY designation",
                [org_id] + args,
            ).fetchall()
        w2 = ""
        eargs = [org_id]
        if fiscal_year:
            w2 = " AND spent_at >= ? AND spent_at < ?"
            start = time.mktime((fiscal_year, 1, 1, 0, 0, 0, 0, 0, 0))
            end = time.mktime((fiscal_year + 1, 1, 1, 0, 0, 0, 0, 0, 0))
            eargs += [start, end]
        expenses = conn.execute(
            "SELECT fund, SUM(amount) total FROM expenses WHERE org_id=?" + w2 + " GROUP BY fund",
            eargs,
        ).fetchall()
    funds = {}
    for r in revenue:
        funds.setdefault(r["fund"], {"revenue": 0.0, "expenses": 0.0})
        funds[r["fund"]]["revenue"] += r["total"]
    for e in expenses:
        funds.setdefault(e["fund"], {"revenue": 0.0, "expenses": 0.0})
        funds[e["fund"]]["expenses"] += e["total"]
    return {
        "service": SERVICE_NAME,
        "fiscal_year": fiscal_year,
        "funds": [
            {
                "fund": f,
                "revenue": v["revenue"],
                "expenses": v["expenses"],
                "net": v["revenue"] - v["expenses"],
            }
            for f, v in funds.items()
        ],
        "total_net": sum(v["revenue"] - v["expenses"] for v in funds.values()),
    }


@app.get("/orgs/{org_id}/reports/functional-expenses")
def report_functional(org_id: str, user: str = Depends(current_user)):
    """Functional expense report: program vs admin vs fundraising."""
    with db() as conn:
        require_org(conn, org_id)
        areas = conn.execute(
            "SELECT functional_area, SUM(amount) total FROM expenses" " WHERE org_id=? GROUP BY functional_area",
            (org_id,),
        ).fetchall()
        total = sum(a["total"] for a in areas)
    return {
        "service": SERVICE_NAME,
        "areas": rows(areas),
        "total": total,
        "program_ratio": (
            next((a["total"] for a in areas if a["functional_area"] == "program"), 0) / total if total else 0
        ),
    }


@app.get("/orgs/{org_id}/reports/position")
def report_position(org_id: str, user: str = Depends(current_user)):
    """Statement of financial position: assets, liabilities and net
    assets (with cash-flow approximation from donations - expenses)."""
    with db() as conn:
        require_org(conn, org_id)
        assets = conn.execute(
            "SELECT COALESCE(SUM(amount),0) t FROM balance_items" " WHERE org_id=? AND kind='asset'",
            (org_id,),
        ).fetchone()["t"]
        liabilities = conn.execute(
            "SELECT COALESCE(SUM(amount),0) t FROM balance_items" " WHERE org_id=? AND kind='liability'",
            (org_id,),
        ).fetchone()["t"]
        donated = (
            conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM donations WHERE org_id=?",
                (org_id,),
            ).fetchone()["t"]
            + conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM revenues WHERE org_id=?",
                (org_id,),
            ).fetchone()["t"]
        )
        spent = conn.execute(
            "SELECT COALESCE(SUM(amount),0) t FROM expenses WHERE org_id=?",
            (org_id,),
        ).fetchone()["t"]
    net_assets = assets + donated - spent - liabilities
    return {
        "service": SERVICE_NAME,
        "assets": assets,
        "liabilities": liabilities,
        "net_assets": net_assets,
    }


@app.get("/orgs/{org_id}/reports/budget-vs-actual")
def report_budget(org_id: str, user: str = Depends(current_user), fiscal_year: Optional[int] = None):
    with db() as conn:
        require_org(conn, org_id)
        q = "SELECT * FROM budgets WHERE org_id=?"
        args = [org_id]
        if fiscal_year:
            q += " AND fiscal_year=?"
            args.append(fiscal_year)
        budget_rows = rows(conn.execute(q, args))
        actuals = {}
        for e in conn.execute(
            "SELECT fund, program, SUM(amount) t FROM expenses" " WHERE org_id=? GROUP BY fund, program",
            (org_id,),
        ).fetchall():
            actuals[(e["fund"], e["program"])] = e["t"]
    lines = []
    for b in budget_rows:
        key = (b["fund"], b["program"])
        actual = actuals.get(key, 0)
        lines.append(
            {
                "fund": b["fund"],
                "program": b["program"],
                "budgeted": b["budgeted"],
                "actual": actual,
                "variance": b["budgeted"] - actual,
                "utilization": (actual / b["budgeted"]) if b["budgeted"] else 0,
            }
        )
    return {"service": SERVICE_NAME, "lines": lines}


@app.get("/orgs/{org_id}/reports/consolidated")
def report_consolidated(org_id: str, user: str = Depends(current_user)):
    """Federation view: per-branch donation and expense totals with the
    org-wide consolidation. Available once the org hits large scale."""
    with db() as conn:
        org = require_org(conn, org_id)
        if org["size_band"] not in ("large", "extra_large"):
            raise HTTPException(
                status_code=403,
                detail="Consolidated reporting unlocks at large scale",
            )
        branches = rows(conn.execute("SELECT id, name FROM branches WHERE org_id=?", (org_id,)))
        per_branch = []
        for b in branches:
            d = conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM donations" " WHERE org_id=? AND branch_id=?",
                (org_id, b["id"]),
            ).fetchone()["t"]
            e = conn.execute(
                "SELECT COALESCE(SUM(amount),0) t FROM expenses" " WHERE org_id=? AND branch_id=?",
                (org_id, b["id"]),
            ).fetchone()["t"]
            per_branch.append(
                {
                    "branch": b["name"],
                    "donations": d,
                    "expenses": e,
                    "net": d - e,
                }
            )
        hq_d = conn.execute(
            "SELECT COALESCE(SUM(amount),0) t FROM donations" " WHERE org_id=? AND branch_id IS NULL",
            (org_id,),
        ).fetchone()["t"]
        hq_e = conn.execute(
            "SELECT COALESCE(SUM(amount),0) t FROM expenses" " WHERE org_id=? AND branch_id IS NULL",
            (org_id,),
        ).fetchone()["t"]
    total_d = sum(x["donations"] for x in per_branch) + hq_d
    total_e = sum(x["expenses"] for x in per_branch) + hq_e
    return {
        "service": SERVICE_NAME,
        "size_band": org["size_band"],
        "hq": {"donations": hq_d, "expenses": hq_e},
        "branches": per_branch,
        "consolidated": {"donations": total_d, "expenses": total_e, "net": total_d - total_e},
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
