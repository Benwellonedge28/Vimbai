"""Vimbai Personal Finance Service - Personal accounting and financial
management for individuals and households. Port: 9030.

Completes the personal tier of Vimbai:
* Recurring transactions: bills and income that repeat on a schedule,
  with due tracking and one-tap recording into the cashbook
* Debts: personal loans, mortgages and credit cards with a real
  amortizing ledger (daily interest accrual), payoff projections and
  extra-payment tracking
* Investments: holdings with weighted-average cost, buy/sell trades and
  live portfolio value
* Tax estimation: progressive brackets (editable per user) with
  liability, effective and marginal rates

All data is per-user (X-User-ID) and optionally linked to a Book.
"""

import os
import sqlite3
import time
import uuid
from contextlib import contextmanager
from datetime import date, datetime, timedelta
from typing import List, Optional

import structlog
from fastapi import Depends, FastAPI, Header, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field, ValidationError

SERVICE_NAME = "personal-finance-service"
SERVICE_VERSION = "1.0.0"
DB_PATH = os.environ.get("PERSONAL_FINANCE_DB", "vimbai_personal_finance.db")
PORT = int(os.environ.get("PORT", "9030"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(
    title="Vimbai Personal Finance Service",
    version=SERVICE_VERSION,
    docs_url="/docs",
)
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

FREQUENCIES = {"weekly", "monthly", "quarterly", "yearly"}
DEBT_KINDS = {"mortgage", "loan", "credit_card", "other"}
ASSET_CLASSES = {"equity", "bond", "etf", "unit_trust", "crypto", "cash", "other"}

# Default tax brackets (annual, rate applies to income within the band).
# Users can replace these with their own jurisdiction's bands.
DEFAULT_TAX_BRACKETS = [
    {"up_to": 10000, "rate": 0.0},
    {"up_to": 30000, "rate": 0.15},
    {"up_to": 70000, "rate": 0.25},
    {"up_to": None, "rate": 0.35},
]

SCHEMA = """
CREATE TABLE IF NOT EXISTS recurring (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    book_id TEXT,
    kind TEXT NOT NULL,
    description TEXT NOT NULL,
    amount REAL NOT NULL,
    currency TEXT DEFAULT 'USD',
    frequency TEXT NOT NULL,
    next_due TEXT NOT NULL,
    last_run TEXT,
    auto_record INTEGER DEFAULT 1,
    active INTEGER DEFAULT 1,
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS debts (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    book_id TEXT,
    name TEXT NOT NULL,
    kind TEXT DEFAULT 'loan',
    principal REAL NOT NULL,
    annual_rate REAL NOT NULL,
    term_months INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    currency TEXT DEFAULT 'USD',
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS payments (
    id TEXT PRIMARY KEY,
    debt_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    amount REAL NOT NULL,
    paid_at TEXT NOT NULL,
    note TEXT DEFAULT '',
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS investments (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    book_id TEXT,
    name TEXT NOT NULL,
    asset_class TEXT DEFAULT 'equity',
    units REAL NOT NULL DEFAULT 0,
    avg_cost REAL NOT NULL DEFAULT 0,
    currency TEXT DEFAULT 'USD',
    last_price REAL DEFAULT 0,
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS trades (
    id TEXT PRIMARY KEY,
    investment_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    side TEXT NOT NULL,
    units REAL NOT NULL,
    price REAL NOT NULL,
    proceeds REAL,
    realized_gain REAL,
    traded_at TEXT NOT NULL,
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS goals (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    target_amount REAL NOT NULL,
    current_amount REAL NOT NULL DEFAULT 0,
    target_date TEXT,
    category TEXT DEFAULT 'savings',
    priority INTEGER DEFAULT 3,
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS income_sources (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    source TEXT NOT NULL,
    amount REAL NOT NULL,
    frequency TEXT DEFAULT 'monthly',
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS debts_legacy (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    creditor TEXT NOT NULL,
    balance REAL NOT NULL,
    interest_rate REAL NOT NULL,
    min_payment REAL NOT NULL,
    kind TEXT DEFAULT 'credit_card',
    created_at REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS tax_brackets (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    up_to REAL,
    rate REAL NOT NULL,
    created_at REAL NOT NULL
);
"""


@contextmanager
def db():
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    try:
        yield conn
        conn.commit()
    finally:
        conn.close()


def row(r):
    return dict(r) if r is not None else None


def rows(rs):
    return [dict(r) for r in rs]


def current_user(x_user_id: Optional[str] = Header(default=None)) -> str:
    if not x_user_id:
        raise HTTPException(status_code=401, detail="X-User-ID header required")
    return x_user_id


def init_db():
    with db() as conn:
        conn.executescript(SCHEMA)


init_db()

# ---------------------------------------------------------------------------
# Recurring transactions
# ---------------------------------------------------------------------------


def _advance(due: str, frequency: str) -> str:
    dt = date.fromisoformat(due)
    if frequency == "weekly":
        nxt = dt + timedelta(weeks=1)
    elif frequency == "monthly":
        month = dt.month % 12 + 1
        year = dt.year + (1 if dt.month == 12 else 0)
        nxt = date(year, month, min(dt.day, 28))
    elif frequency == "quarterly":
        month = dt.month + 3
        year = dt.year + (month - 1) // 12
        month = (month - 1) % 12 + 1
        nxt = date(year, month, min(dt.day, 28))
    else:  # yearly
        nxt = date(dt.year + 1, dt.month, min(dt.day, 28))
    return nxt.isoformat()


class RecurringCreate(BaseModel):
    kind: str  # bill | income
    description: str = Field(min_length=1, max_length=200)
    amount: float = Field(gt=0)
    currency: str = "USD"
    frequency: str
    next_due: str  # ISO date
    book_id: str = ""
    auto_record: bool = True


class RecurringUpdate(BaseModel):
    amount: Optional[float] = None
    frequency: Optional[str] = None
    next_due: Optional[str] = None
    active: Optional[bool] = None
    description: Optional[str] = None


def _recurring_or_404(conn, rec_id: str, user: str):
    r = conn.execute("SELECT * FROM recurring WHERE id=? AND user_id=?", (rec_id, user)).fetchone()
    if r is None:
        raise HTTPException(status_code=404, detail="Unknown recurring transaction")
    return r


@app.post("/recurring")
def create_recurring(body: RecurringCreate, user: str = Depends(current_user)):
    if body.kind not in ("bill", "income"):
        raise HTTPException(status_code=400, detail="kind must be bill or income")
    if body.frequency not in FREQUENCIES:
        raise HTTPException(
            status_code=400,
            detail="frequency must be %s" % sorted(FREQUENCIES),
        )
    try:
        date.fromisoformat(body.next_due)
    except ValueError:
        raise HTTPException(status_code=400, detail="next_due must be an ISO date")
    rid = str(uuid.uuid4())
    with db() as conn:
        conn.execute(
            "INSERT INTO recurring (id, user_id, book_id, kind, description,"
            " amount, currency, frequency, next_due, auto_record, active,"
            " created_at) VALUES (?,?,?,?,?,?,?,?,?,?,1,?)",
            (
                rid,
                user,
                body.book_id or None,
                body.kind,
                body.description,
                body.amount,
                body.currency,
                body.frequency,
                body.next_due,
                1 if body.auto_record else 0,
                time.time(),
            ),
        )
        r = _recurring_or_404(conn, rid, user)
    return {"service": SERVICE_NAME, "recurring": row(r)}


@app.get("/recurring")
def list_recurring(book_id: str = "", user: str = Depends(current_user)):
    today = date.today().isoformat()
    with db() as conn:
        if book_id:
            rs = conn.execute(
                "SELECT * FROM recurring WHERE user_id=? AND book_id=?" " AND active=1 ORDER BY next_due",
                (user, book_id),
            ).fetchall()
        else:
            rs = conn.execute(
                "SELECT * FROM recurring WHERE user_id=? AND active=1" " ORDER BY next_due",
                (user,),
            ).fetchall()
    out = []
    for r in rows(rs):
        r["due"] = bool(r["next_due"] <= today)
        out.append(r)
    return {"service": SERVICE_NAME, "recurring": out, "today": today}


@app.patch("/recurring/{rec_id}")
def update_recurring(rec_id: str, body: RecurringUpdate, user: str = Depends(current_user)):
    with db() as conn:
        _recurring_or_404(conn, rec_id, user)
        sets, vals = [], []
        if body.amount is not None:
            sets.append("amount=?")
            vals.append(body.amount)
        if body.frequency is not None:
            if body.frequency not in FREQUENCIES:
                raise HTTPException(status_code=400, detail="bad frequency")
            sets.append("frequency=?")
            vals.append(body.frequency)
        if body.next_due is not None:
            sets.append("next_due=?")
            vals.append(body.next_due)
        if body.description is not None:
            sets.append("description=?")
            vals.append(body.description)
        if body.active is not None:
            sets.append("active=?")
            vals.append(1 if body.active else 0)
        if not sets:
            raise HTTPException(status_code=400, detail="nothing to update")
        vals.append(rec_id)
        vals.append(user)
        conn.execute(
            "UPDATE recurring SET %s WHERE id=? AND user_id=?" % ",".join(sets),
            vals,
        )
        r = _recurring_or_404(conn, rec_id, user)
    return {"service": SERVICE_NAME, "recurring": row(r)}


@app.delete("/recurring/{rec_id}")
def delete_recurring(rec_id: str, user: str = Depends(current_user)):
    with db() as conn:
        _recurring_or_404(conn, rec_id, user)
        conn.execute(
            "UPDATE recurring SET active=0 WHERE id=? AND user_id=?",
            (rec_id, user),
        )
    return {"service": SERVICE_NAME, "status": "deactivated"}


@app.post("/recurring/{rec_id}/run")
def run_recurring(rec_id: str, user: str = Depends(current_user)):
    """Record this recurring transaction now (the client posts the returned
    payload to its cashbook), then advance to the next due date."""
    with db() as conn:
        r = _recurring_or_404(conn, rec_id, user)
        today = date.today().isoformat()
        nxt = _advance(r["next_due"], r["frequency"])
        conn.execute(
            "UPDATE recurring SET last_run=?, next_due=? WHERE id=?",
            (today, nxt, rec_id),
        )
        result = {
            "kind": r["kind"],
            "description": r["description"],
            "amount": r["amount"],
            "currency": r["currency"],
            "occurred_on": today,
            "book_id": r["book_id"],
        }
    return {"service": SERVICE_NAME, "recorded": result, "next_due": nxt}


# ---------------------------------------------------------------------------
# Debts (amortizing ledger)
# ---------------------------------------------------------------------------


class DebtCreate(BaseModel):
    name: str = Field(min_length=1, max_length=120)
    kind: str = "loan"
    principal: float = Field(gt=0)
    annual_rate: float = Field(ge=0)  # percent
    term_months: int = Field(gt=0)
    started_at: str  # ISO date
    currency: str = "USD"
    book_id: str = ""


class PaymentCreate(BaseModel):
    amount: float = Field(gt=0)
    paid_at: Optional[str] = None  # ISO datetime, default now
    note: str = ""


def _monthly_payment(principal: float, annual_rate: float, term_months: int) -> float:
    r = annual_rate / 100.0 / 12.0
    if r == 0:
        return principal / term_months
    return principal * r / (1 - (1 + r) ** (-term_months))


def _parse_dt(s: str) -> datetime:
    return datetime.fromisoformat(s)


def _debt_or_404(conn, debt_id: str, user: str):
    d = conn.execute("SELECT * FROM debts WHERE id=? AND user_id=?", (debt_id, user)).fetchone()
    if d is None:
        raise HTTPException(status_code=404, detail="Unknown debt")
    return d


def _debt_state(conn, d) -> dict:
    """Replay the payment ledger: each payment accrues daily interest since
    the previous payment (or the loan start) before reducing the balance."""
    pmts = conn.execute(
        "SELECT * FROM payments WHERE debt_id=? ORDER BY paid_at",
        (d["id"],),
    ).fetchall()
    start = _parse_dt(d["started_at"])
    balance = d["principal"]
    total_interest = 0.0
    total_paid = 0.0
    daily_rate = d["annual_rate"] / 100.0 / 365.0
    for p in pmts:
        paid_at = _parse_dt(p["paid_at"])
        days = max((paid_at - start).total_seconds() / 86400.0, 0.0)
        interest = balance * daily_rate * days
        principal_part = max(p["amount"] - interest, 0.0)
        balance = max(balance - principal_part, 0.0)
        total_interest += min(interest, p["amount"])
        total_paid += p["amount"]
        start = paid_at
    scheduled = _monthly_payment(d["principal"], d["annual_rate"], d["term_months"])
    months_left = balance / scheduled if scheduled > 0 else 0
    payoff = date.today() + timedelta(days=30.5 * months_left)
    return {
        "balance": round(balance, 2),
        "scheduled_monthly_payment": round(scheduled, 2),
        "total_paid": round(total_paid, 2),
        "total_interest_paid": round(total_interest, 2),
        "months_remaining": round(months_left, 1),
        "projected_payoff": payoff.isoformat(),
        "payments_recorded": len(pmts),
    }


@app.post("/debts")
def create_debt(body: dict, x_user_id: Optional[str] = Header(default=None)):
    """New-style: DebtCreate JSON with the owner's X-User-ID. Legacy: a
    DebtItem JSON (creditor/balance/min_payment) is stored under its own
    user_id, preserving the old in-memory API (now backed by SQLite)."""
    if "creditor" in body:
        try:
            item = DebtItem(**body)
        except ValidationError as e:
            raise HTTPException(status_code=422, detail=str(e))
        with db() as conn:
            conn.execute(
                "INSERT INTO debts_legacy (id, user_id, creditor, balance,"
                " interest_rate, min_payment, kind, created_at)"
                " VALUES (?,?,?,?,?,?,?,?)",
                (
                    item.id,
                    item.user_id,
                    item.creditor,
                    item.balance,
                    item.interest_rate,
                    item.min_payment,
                    item.type,
                    time.time(),
                ),
            )
        return item.model_dump(mode="json")
    if not x_user_id:
        raise HTTPException(status_code=401, detail="X-User-ID header required")
    try:
        parsed = DebtCreate(**body)
    except ValidationError as e:
        raise HTTPException(status_code=422, detail=str(e))
    return _create_new_debt(parsed, x_user_id)


def _create_new_debt(body: DebtCreate, user: str):
    if body.kind not in DEBT_KINDS:
        raise HTTPException(status_code=400, detail="kind must be %s" % sorted(DEBT_KINDS))
    try:
        date.fromisoformat(body.started_at)
    except ValueError:
        raise HTTPException(status_code=400, detail="started_at must be an ISO date")
    did = str(uuid.uuid4())
    with db() as conn:
        conn.execute(
            "INSERT INTO debts (id, user_id, book_id, name, kind, principal,"
            " annual_rate, term_months, started_at, currency, created_at)"
            " VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            (
                did,
                user,
                body.book_id or None,
                body.name,
                body.kind,
                body.principal,
                body.annual_rate,
                body.term_months,
                body.started_at,
                body.currency,
                time.time(),
            ),
        )
        d = _debt_or_404(conn, did, user)
        state = _debt_state(conn, d)
    return {"service": SERVICE_NAME, "debt": row(d), "state": state}


@app.get("/debts")
def list_debts(user: str = Depends(current_user)):
    with db() as conn:
        ds = conn.execute(
            "SELECT * FROM debts WHERE user_id=? ORDER BY created_at DESC",
            (user,),
        ).fetchall()
        out = []
        for d in ds:
            state = _debt_state(conn, d)
            out.append({**row(d), "state": state})
    total_balance = round(sum(o["state"]["balance"] for o in out), 2)
    return {"service": SERVICE_NAME, "debts": out, "total_balance": total_balance}


@app.get("/debts/{key}")
def get_debt(key: str, x_user_id: Optional[str] = Header(default=None)):
    """New-style: GET /debts/{debt_id} with the owner's X-User-ID returns the
    debt with its amortization state. Legacy: GET /debts/{user_id} (the old
    in-memory API) returns that user's debt items instead."""
    with db() as conn:
        d = None
        if x_user_id:
            d = conn.execute("SELECT * FROM debts WHERE id=? AND user_id=?", (key, x_user_id)).fetchone()
        if d is not None:
            state = _debt_state(conn, d)
            return {"service": SERVICE_NAME, "debt": row(d), "state": state}
        # Legacy behavior: treat the path segment as a user id
        items = rows(
            conn.execute(
                "SELECT * FROM debts_legacy WHERE user_id=? ORDER BY created_at",
                (key,),
            ).fetchall()
        )
        total_min = 0.0
        debts = []
        for it in items:
            total_min += it["min_payment"]
            debts.append(
                {
                    "id": it["id"],
                    "user_id": it["user_id"],
                    "creditor": it["creditor"],
                    "balance": it["balance"],
                    "interest_rate": it["interest_rate"],
                    "min_payment": it["min_payment"],
                    "type": it["kind"],
                }
            )
        total_debt = sum(d["balance"] for d in debts)
        for nd in conn.execute("SELECT * FROM debts WHERE user_id=? ORDER BY created_at", (key,)).fetchall():
            state = _debt_state(conn, nd)
            total_debt += state["balance"]
            total_min += state["scheduled_monthly_payment"]
            debts.append(
                {
                    "id": nd["id"],
                    "user_id": key,
                    "creditor": nd["name"],
                    "balance": state["balance"],
                    "interest_rate": nd["annual_rate"],
                    "min_payment": state["scheduled_monthly_payment"],
                    "type": nd["kind"],
                }
            )
    return {
        "user_id": key,
        "debts": debts,
        "total_debt": round(total_debt, 2),
        "total_min_payments": round(total_min, 2),
    }


@app.post("/debts/{debt_id}/payments")
def add_payment(debt_id: str, body: PaymentCreate, user: str = Depends(current_user)):
    paid_at = body.paid_at or datetime.utcnow().isoformat()
    try:
        _parse_dt(paid_at)
    except ValueError:
        raise HTTPException(status_code=400, detail="paid_at must be ISO datetime")
    with db() as conn:
        d = _debt_or_404(conn, debt_id, user)
        pid = str(uuid.uuid4())
        conn.execute(
            "INSERT INTO payments (id, debt_id, user_id, amount, paid_at," " note, created_at) VALUES (?,?,?,?,?,?,?)",
            (pid, debt_id, user, body.amount, paid_at, body.note, time.time()),
        )
        state = _debt_state(conn, d)
    return {"service": SERVICE_NAME, "payment_id": pid, "state": state}


@app.get("/debts/{debt_id}/schedule")
def debt_schedule(debt_id: str, user: str = Depends(current_user)):
    """Projection from the current balance at the scheduled monthly payment."""
    with db() as conn:
        d = _debt_or_404(conn, debt_id, user)
        state = _debt_state(conn, d)
    balance = state["balance"]
    pmt = state["scheduled_monthly_payment"]
    r = d["annual_rate"] / 100.0 / 12.0
    schedule = []
    month = 0
    while balance > 0.005 and month < 1200:
        month += 1
        interest = balance * r
        principal_part = min(pmt - interest, balance)
        if principal_part <= 0:
            raise HTTPException(
                status_code=400,
                detail="Payment too small to cover interest - debt never ends",
            )
        balance -= principal_part
        schedule.append(
            {
                "month": month,
                "payment": round(interest + principal_part, 2),
                "interest": round(interest, 2),
                "principal": round(principal_part, 2),
                "balance": round(balance, 2),
            }
        )
    total_interest = sum(s["interest"] for s in schedule)
    return {
        "service": SERVICE_NAME,
        "current_balance": state["balance"],
        "monthly_payment": pmt,
        "months": len(schedule),
        "total_interest_remaining": round(total_interest, 2),
        "total_remaining": round(state["balance"] + total_interest, 2),
        "schedule": schedule,
    }


# ---------------------------------------------------------------------------
# Investments
# ---------------------------------------------------------------------------


class InvestmentCreate(BaseModel):
    name: str = Field(min_length=1, max_length=120)
    asset_class: str = "equity"
    currency: str = "USD"
    book_id: str = ""
    initial_units: float = 0
    initial_price: float = 0


class TradeBody(BaseModel):
    side: str  # buy | sell
    units: float = Field(gt=0)
    price: float = Field(gt=0)
    traded_at: Optional[str] = None


class PriceBody(BaseModel):
    price: float = Field(gt=0)


def _investment_or_404(conn, inv_id: str, user: str):
    i = conn.execute("SELECT * FROM investments WHERE id=? AND user_id=?", (inv_id, user)).fetchone()
    if i is None:
        raise HTTPException(status_code=404, detail="Unknown investment")
    return i


@app.post("/investments")
def create_investment(body: InvestmentCreate, user: str = Depends(current_user)):
    if body.asset_class not in ASSET_CLASSES:
        raise HTTPException(status_code=400, detail="asset_class must be %s" % sorted(ASSET_CLASSES))
    iid = str(uuid.uuid4())
    with db() as conn:
        cost = body.initial_units * body.initial_price
        conn.execute(
            "INSERT INTO investments (id, user_id, book_id, name, asset_class,"
            " units, avg_cost, currency, last_price, created_at)"
            " VALUES (?,?,?,?,?,?,?,?,?,?)",
            (
                iid,
                user,
                body.book_id or None,
                body.name,
                body.asset_class,
                body.initial_units,
                body.initial_price if body.initial_units else 0,
                body.currency,
                body.initial_price,
                time.time(),
            ),
        )
        if body.initial_units > 0:
            conn.execute(
                "INSERT INTO trades (id, investment_id, user_id, side, units,"
                " price, proceeds, realized_gain, traded_at, created_at)"
                " VALUES (?,?,?,?,?,?,NULL,NULL,?,?)",
                (
                    str(uuid.uuid4()),
                    iid,
                    user,
                    "buy",
                    body.initial_units,
                    body.initial_price,
                    datetime.utcnow().isoformat(),
                    time.time(),
                ),
            )
        i = _investment_or_404(conn, iid, user)
    return {"service": SERVICE_NAME, "investment": row(i), "cost": round(cost, 2)}


@app.get("/investments")
def list_investments(user: str = Depends(current_user)):
    with db() as conn:
        rs = conn.execute(
            "SELECT * FROM investments WHERE user_id=? ORDER BY created_at DESC",
            (user,),
        ).fetchall()
    out = []
    for r in rows(rs):
        value = r["units"] * r["last_price"]
        cost = r["units"] * r["avg_cost"]
        r["market_value"] = round(value, 2)
        r["cost_basis"] = round(cost, 2)
        r["unrealized_gain"] = round(value - cost, 2)
        r["gain_pct"] = round((value - cost) / cost * 100, 2) if cost else 0.0
        out.append(r)
    total_value = round(sum(o["market_value"] for o in out), 2)
    total_cost = round(sum(o["cost_basis"] for o in out), 2)
    return {
        "service": SERVICE_NAME,
        "investments": out,
        "portfolio": {
            "market_value": total_value,
            "cost_basis": total_cost,
            "unrealized_gain": round(total_value - total_cost, 2),
        },
    }


@app.post("/investments/{inv_id}/trades")
def add_trade(inv_id: str, body: TradeBody, user: str = Depends(current_user)):
    if body.side not in ("buy", "sell"):
        raise HTTPException(status_code=400, detail="side must be buy or sell")
    with db() as conn:
        i = _investment_or_404(conn, inv_id, user)
        traded_at = body.traded_at or datetime.utcnow().isoformat()
        if body.side == "buy":
            new_units = i["units"] + body.units
            new_avg = (i["units"] * i["avg_cost"] + body.units * body.price) / new_units if new_units else 0.0
            realized = None
            proceeds = None
        else:
            if body.units > i["units"] + 1e-9:
                raise HTTPException(status_code=400, detail="Cannot sell more units than held")
            new_units = i["units"] - body.units
            new_avg = i["avg_cost"]
            proceeds = round(body.units * body.price, 2)
            realized = round(body.units * (body.price - i["avg_cost"]), 2)
        conn.execute(
            "UPDATE investments SET units=?, avg_cost=?, last_price=? WHERE id=?",
            (new_units, new_avg, body.price, inv_id),
        )
        conn.execute(
            "INSERT INTO trades (id, investment_id, user_id, side, units,"
            " price, proceeds, realized_gain, traded_at, created_at)"
            " VALUES (?,?,?,?,?,?,?,?,?,?)",
            (
                str(uuid.uuid4()),
                inv_id,
                user,
                body.side,
                body.units,
                body.price,
                proceeds,
                realized,
                traded_at,
                time.time(),
            ),
        )
        i = _investment_or_404(conn, inv_id, user)
    return {
        "service": SERVICE_NAME,
        "investment": row(i),
        "realized_gain": realized,
        "proceeds": proceeds,
    }


@app.post("/investments/{inv_id}/price")
def update_price(inv_id: str, body: PriceBody, user: str = Depends(current_user)):
    with db() as conn:
        _investment_or_404(conn, inv_id, user)
        conn.execute("UPDATE investments SET last_price=? WHERE id=?", (body.price, inv_id))
        i = _investment_or_404(conn, inv_id, user)
    return {"service": SERVICE_NAME, "investment": row(i)}


# ---------------------------------------------------------------------------
# Tax estimation
# ---------------------------------------------------------------------------


class BracketIn(BaseModel):
    up_to: Optional[float]  # None = top band
    rate: float = Field(ge=0, le=1)  # 0.15 = 15%


class BracketsBody(BaseModel):
    brackets: List[BracketIn]


class TaxEstimateBody(BaseModel):
    annual_income: float = Field(ge=0)
    other_income: float = 0.0
    deductions: float = 0.0
    paye_paid: float = 0.0


def _load_brackets(conn, user: str):
    bs = conn.execute(
        "SELECT up_to, rate FROM tax_brackets WHERE user_id=?" " ORDER BY up_to IS NULL, up_to",
        (user,),
    ).fetchall()
    if not bs:
        return [dict(b) for b in DEFAULT_TAX_BRACKETS]
    return [{"up_to": b["up_to"], "rate": b["rate"]} for b in bs]


def _tax_from_brackets(taxable: float, brackets):
    tax = 0.0
    lower = 0.0
    marginal = 0.0
    for b in brackets:
        upper = b["up_to"]
        rate = b["rate"]
        if taxable > lower:
            band_top = taxable if upper is None else min(taxable, upper)
            tax += (band_top - lower) * rate
            marginal = rate
        if upper is None or taxable <= upper:
            break
        lower = upper
    return tax, marginal


@app.get("/tax/brackets")
def get_brackets(user: str = Depends(current_user)):
    with db() as conn:
        brackets = _load_brackets(conn, user)
    return {"service": SERVICE_NAME, "brackets": brackets}


@app.put("/tax/brackets")
def put_brackets(body: BracketsBody, user: str = Depends(current_user)):
    if not body.brackets:
        raise HTTPException(status_code=400, detail="brackets cannot be empty")
    rates = [b.rate for b in body.brackets]
    if rates != sorted(rates):
        raise HTTPException(status_code=400, detail="brackets must be ordered by rate")
    ups = [b.up_to for b in body.brackets if b.up_to is not None]
    if ups != sorted(ups):
        raise HTTPException(status_code=400, detail="band upper limits must ascend")
    if body.brackets[-1].up_to is not None:
        raise HTTPException(status_code=400, detail="final band must be the top band (up_to null)")
    with db() as conn:
        conn.execute("DELETE FROM tax_brackets WHERE user_id=?", (user,))
        for b in body.brackets:
            conn.execute(
                "INSERT INTO tax_brackets (id, user_id, up_to, rate, created_at)" " VALUES (?,?,?,?,?)",
                (str(uuid.uuid4()), user, b.up_to, b.rate, time.time()),
            )
        brackets = _load_brackets(conn, user)
    return {"service": SERVICE_NAME, "brackets": brackets}


@app.post("/tax/estimate")
def tax_estimate(body: TaxEstimateBody, user: str = Depends(current_user)):
    gross = body.annual_income + body.other_income
    taxable = max(gross - body.deductions, 0.0)
    with db() as conn:
        brackets = _load_brackets(conn, user)
    tax, marginal = _tax_from_brackets(taxable, brackets)
    return {
        "service": SERVICE_NAME,
        "gross_income": round(gross, 2),
        "taxable_income": round(taxable, 2),
        "estimated_tax": round(tax, 2),
        "effective_rate": round(tax / gross, 4) if gross else 0.0,
        "marginal_rate": marginal,
        "net_after_tax": round(gross - tax, 2),
        "monthly_withholding": round(tax / 12, 2),
        "balance_due": round(tax - body.paye_paid, 2),
    }


# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# Legacy compatibility API (the original in-memory personal finance
# endpoints: goals, income, legacy debt items, overview) - now DB-backed
# ---------------------------------------------------------------------------


class FinancialGoal(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    name: str
    target_amount: float
    current_amount: float = 0
    target_date: Optional[str] = None
    category: str = "savings"
    priority: int = 3


class IncomeSource(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    source: str
    amount: float
    frequency: str = "monthly"


class DebtItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    creditor: str
    balance: float
    interest_rate: float
    min_payment: float
    type: str = "credit_card"


@app.post("/goals")
def create_goal(goal: FinancialGoal):
    with db() as conn:
        conn.execute(
            "INSERT INTO goals (id, user_id, name, target_amount,"
            " current_amount, target_date, category, priority, created_at)"
            " VALUES (?,?,?,?,?,?,?,?,?)",
            (
                goal.id,
                goal.user_id,
                goal.name,
                goal.target_amount,
                goal.current_amount,
                goal.target_date,
                goal.category,
                goal.priority,
                time.time(),
            ),
        )
    return goal.model_dump(mode="json")


@app.get("/goals/{user_id}")
def get_goals(user_id: str):
    with db() as conn:
        gs = rows(
            conn.execute(
                "SELECT * FROM goals WHERE user_id=? ORDER BY created_at",
                (user_id,),
            ).fetchall()
        )
    total = len(gs)
    progress_avg = sum(g["current_amount"] / max(1, g["target_amount"]) for g in gs) / max(1, total)
    return {"user_id": user_id, "goals": gs, "total": total, "progress_avg": progress_avg}


@app.post("/income")
def add_income(income: IncomeSource):
    with db() as conn:
        conn.execute(
            "INSERT INTO income_sources (id, user_id, source, amount," " frequency, created_at) VALUES (?,?,?,?,?,?)",
            (
                income.id,
                income.user_id,
                income.source,
                income.amount,
                income.frequency,
                time.time(),
            ),
        )
    return income.model_dump(mode="json")


@app.get("/income/{user_id}")
def get_income(user_id: str):
    with db() as conn:
        ss = rows(
            conn.execute(
                "SELECT * FROM income_sources WHERE user_id=? ORDER BY created_at",
                (user_id,),
            ).fetchall()
        )
    return {
        "user_id": user_id,
        "sources": ss,
        "total_monthly": sum(s["amount"] for s in ss),
    }


@app.get("/overview/{user_id}")
def financial_overview(user_id: str):
    with db() as conn:
        income_total = conn.execute(
            "SELECT COALESCE(SUM(amount),0) FROM income_sources" " WHERE user_id=?",
            (user_id,),
        ).fetchone()[0]
        legacy_total = conn.execute(
            "SELECT COALESCE(SUM(balance),0) FROM debts_legacy" " WHERE user_id=?",
            (user_id,),
        ).fetchone()[0]
        debt_total = legacy_total
        for nd in conn.execute("SELECT * FROM debts WHERE user_id=?", (user_id,)).fetchall():
            debt_total += _debt_state(conn, nd)["balance"]
        goals = rows(
            conn.execute(
                "SELECT * FROM goals WHERE user_id=? ORDER BY created_at",
                (user_id,),
            ).fetchall()
        )
    return {
        "user_id": user_id,
        "monthly_income": income_total,
        "total_debt": round(debt_total, 2),
        "debt_to_income": debt_total / max(1, income_total * 12) * 100,
        "active_goals": len(goals),
        "goals_progress": sum(g["current_amount"] for g in goals),
        "goals_target": sum(g["target_amount"] for g in goals),
    }


@app.get("/health")
def health():
    return {"service": SERVICE_NAME, "status": "healthy", "version": SERVICE_VERSION}


@app.get("/")
def root():
    return {
        "service": SERVICE_NAME,
        "status": "healthy",
        "version": SERVICE_VERSION,
        "endpoints": [
            "POST /recurring",
            "GET  /recurring",
            "PATCH /recurring/{id}",
            "DELETE /recurring/{id}",
            "POST /recurring/{id}/run",
            "POST /debts",
            "GET  /debts",
            "GET  /debts/{id}",
            "POST /debts/{id}/payments",
            "GET  /debts/{id}/schedule",
            "POST /investments",
            "GET  /investments",
            "POST /investments/{id}/trades",
            "POST /investments/{id}/price",
            "GET  /tax/brackets",
            "PUT  /tax/brackets",
            "POST /tax/estimate",
            "POST /goals",
            "GET  /goals/{user_id}",
            "POST /income",
            "GET  /income/{user_id}",
            "GET  /debts/{user_id}",
            "GET  /overview/{user_id}",
            "GET  /health",
        ],
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
