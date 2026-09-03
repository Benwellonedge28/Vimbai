"""
Vimbai NPO Scale Service
Lifecycle, scaling and donor-grade reporting for non-profits.

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
from fastapi import Depends, FastAPI, Header, HTTPException
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

# Annual revenue (USD) thresholds used for automatic classification.
BAND_REVENUE_THRESHOLDS = [
    ("small", 50_000),
    ("medium", 500_000),
    ("large", 5_000_000),
    ("extra_large", float("inf")),
]

FEATURES_BY_BAND: Dict[str, List[str]] = {
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
    "small": float("inf"),
    "medium": 5_000,
    "large": 2_000,
    "extra_large": 1_000,
}


def classify_band(annual_revenue: float, headcount: int, branches: int) -> str:
    """Automatic size-band classification.

    Revenue is the primary signal; headcount and branch count push an
    organization up a band so growth is never blocked by classification.
    """
    band = "small"
    for name, threshold in BAND_REVENUE_THRESHOLDS:
        if annual_revenue < threshold:
            band = name
            break
    if branches > 25 or headcount > 500:
        band = "extra_large"
    elif branches > 5 or headcount > 50:
        if band in ("small", "medium"):
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
    band = classify_band(org["annual_revenue"] or 0, org["headcount"] or 0, n_branches)
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
    band = classify_band(body.annual_revenue, body.headcount, 0)
    with db() as conn:
        conn.execute(
            "INSERT INTO orgs (id, owner_id, name, sector, country, currency,"
            " size_band, annual_revenue, headcount, created_at, updated_at)"
            " VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            (
                org_id,
                user,
                body.name,
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
        "org": {"id": org_id, "name": body.name, "size_band": band},
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
        features = FEATURES_BY_BAND[org["size_band"]]
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
    return {
        "service": SERVICE_NAME,
        "size_band": org["size_band"],
        "features": FEATURES_BY_BAND[org["size_band"]],
        "approval_limit": APPROVAL_LIMITS[org["size_band"]],
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
        require_org(conn, org_id)
        w, args = _fy_where(fiscal_year)
        revenue = conn.execute(
            "SELECT designation fund, SUM(amount) total FROM donations" " WHERE org_id=?" + w + " GROUP BY designation",
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
        donated = conn.execute(
            "SELECT COALESCE(SUM(amount),0) t FROM donations WHERE org_id=?",
            (org_id,),
        ).fetchone()["t"]
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
