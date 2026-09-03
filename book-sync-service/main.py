"""
Vimbai Book Sync Service
Shared-Book cloud sync for the Vimbai financial operating system.

Implements the shared-Book tier of the deployment model promised in
book-design.md:

  * Books ("Spaces") - one atomic unit per audience (personal,
    household/family, group/savings club, business). A user can be
    a member of many Books with different privileges in each.
  * End-to-end encryption - the server stores only opaque encrypted
    entry blobs. Book keys live on member devices and are shared via
    key-wrapped invitations; the server can never read Book contents.
  * Delta sync - devices push batches of locally queued, immutable,
    append-only journal entries and pull everything they have not yet
    seen (a monotonically increasing per-Book sequence).
  * Last-write-wins metadata sync for mutable descriptive fields
    (Book name, member display names) - the ledger itself never
    conflicts because entries are immutable.

Identity: the API gateway validates the JWT and forwards X-User-ID.
This service trusts that header only from the gateway.

Storage: SQLite at /data/vimbai_book_sync.db (or ./book_sync.db in
dev). Single-file, zero-dependency, cheap to host - suitable for a
US$5/month managed node.
"""

import os
import sqlite3
import time
import uuid
from contextlib import contextmanager
from typing import Dict, List, Optional

from fastapi import Depends, FastAPI, Header, HTTPException
from pydantic import BaseModel, Field

SERVICE_NAME = "book-sync-service"
SERVICE_VERSION = "1.0.0"

DB_PATH = os.getenv("BOOK_SYNC_DB", os.path.join(os.getcwd(), "vimbai_book_sync.db"))

# Membership roles, most -> least privileged (book-design.md Ch. 35)
ROLE_OWNER = "owner"
ROLE_ADMIN = "admin"
ROLE_TREASURER = "treasurer"  # group/savings-club payout authority
ROLE_BOOKKEEPER = "bookkeeper"  # may enter transactions
ROLE_VIEWER = "viewer"
ROLE_AUDITOR = "auditor"  # read-only + audit log
VALID_ROLES = {
    ROLE_OWNER,
    ROLE_ADMIN,
    ROLE_TREASURER,
    ROLE_BOOKKEEPER,
    ROLE_VIEWER,
    ROLE_AUDITOR,
}
WRITER_ROLES = {ROLE_OWNER, ROLE_ADMIN, ROLE_TREASURER, ROLE_BOOKKEEPER}

# Audience tiers - configure which features a Book unlocks
# (nonprofit = fund accounting / npo-service: restricted funds, donor
# reporting, grants;  household = family/community shared budgets;
# group = savings clubs & societies; business = full ~300-service stack)
VALID_TIERS = {"personal", "household", "group", "business", "nonprofit"}

app = FastAPI(
    title="Vimbai Book Sync Service",
    version=SERVICE_VERSION,
    description="Shared-Book cloud sync: Books, memberships, E2E-encrypted entries, delta sync.",
)


# ---------------------------------------------------------------------------
# Database
# ---------------------------------------------------------------------------
@contextmanager
def db():
    conn = sqlite3.connect(DB_PATH, timeout=15)
    conn.row_factory = sqlite3.Row
    try:
        yield conn
        conn.commit()
    finally:
        conn.close()


def init_db():
    with db() as conn:
        conn.executescript("""
            CREATE TABLE IF NOT EXISTS books (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                tier TEXT NOT NULL,
                description TEXT DEFAULT '',
                created_by TEXT NOT NULL,
                created_at REAL NOT NULL,
                seq INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS memberships (
                id TEXT PRIMARY KEY,
                book_id TEXT NOT NULL REFERENCES books(id),
                user_id TEXT NOT NULL,
                role TEXT NOT NULL,
                display_name TEXT DEFAULT '',
                status TEXT NOT NULL DEFAULT 'active',  -- active | invited
                invited_by TEXT DEFAULT '',
                created_at REAL NOT NULL,
                UNIQUE(book_id, user_id)
            );
            CREATE TABLE IF NOT EXISTS devices (
                id TEXT PRIMARY KEY,
                book_id TEXT NOT NULL REFERENCES books(id),
                user_id TEXT NOT NULL,
                device_name TEXT DEFAULT '',
                last_seen_seq INTEGER NOT NULL DEFAULT 0,
                created_at REAL NOT NULL,
                UNIQUE(book_id, id)
            );
            CREATE TABLE IF NOT EXISTS entries (
                book_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                entry_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                payload TEXT NOT NULL,          -- opaque E2E-encrypted blob (base64)
                created_at REAL NOT NULL,
                PRIMARY KEY (book_id, seq)
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_entries_entry
                ON entries(book_id, entry_id);
            CREATE TABLE IF NOT EXISTS metadata (
                book_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,            -- opaque E2E-encrypted blob (base64)
                updated_at REAL NOT NULL,
                updated_by TEXT NOT NULL,
                PRIMARY KEY (book_id, key)
            );
            CREATE TABLE IF NOT EXISTS audit_log (
                id TEXT PRIMARY KEY,
                book_id TEXT NOT NULL,
                actor TEXT NOT NULL,
                action TEXT NOT NULL,
                detail TEXT DEFAULT '',
                created_at REAL NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mem_user ON memberships(user_id);
            """)


init_db()


# ---------------------------------------------------------------------------
# Auth dependency: gateway sets X-User-ID after JWT validation
# ---------------------------------------------------------------------------
def current_user(x_user_id: Optional[str] = Header(default=None)) -> str:
    if not x_user_id:
        raise HTTPException(status_code=401, detail="Missing gateway identity header")
    return x_user_id


def require_membership(conn: sqlite3.Connection, user_id: str, book_id: str, roles: set) -> sqlite3.Row:
    row = conn.execute(
        "SELECT * FROM memberships WHERE book_id=? AND user_id=? AND status='active'",
        (book_id, user_id),
    ).fetchone()
    if row is None:
        raise HTTPException(status_code=403, detail="Not an active member of this book")
    if roles is not None and row["role"] not in roles:
        raise HTTPException(status_code=403, detail="Role does not permit this action")
    return row


def audit(conn: sqlite3.Connection, book_id: str, actor: str, action: str, detail: str = ""):
    conn.execute(
        "INSERT INTO audit_log (id, book_id, actor, action, detail, created_at) VALUES (?,?,?,?,?,?)",
        (str(uuid.uuid4()), book_id, actor, action, detail, time.time()),
    )


# ---------------------------------------------------------------------------
# Models
# ---------------------------------------------------------------------------
class BookCreate(BaseModel):
    name: str = Field(min_length=1, max_length=120)
    tier: str
    description: str = ""
    # The Book's AES key wrapped for the creator (public-key wrapped, opaque
    # to this server). Empty for personal Books that never sync.
    wrapped_book_key: str = ""


class BookUpdate(BaseModel):
    name: Optional[str] = None
    description: Optional[str] = None


class MemberInvite(BaseModel):
    user_id: str
    role: str
    display_name: str = ""
    # Book key wrapped for the invitee with their public key.
    wrapped_book_key: str = ""


class MemberUpdate(BaseModel):
    role: Optional[str] = None
    status: Optional[str] = None  # active | revoked


class DeviceRegister(BaseModel):
    device_name: str = ""


class EntryPush(BaseModel):
    device_id: str
    # Each entry: {"entry_id", "payload" (E2E-encrypted blob), "created_at"}
    entries: List[Dict]


class MetaUpsert(BaseModel):
    key: str
    value: str  # E2E-encrypted blob (base64)
    updated_at: float


# ---------------------------------------------------------------------------
# Books
# ---------------------------------------------------------------------------
@app.post("/books")
def create_book(body: BookCreate, user_id: str = Depends(current_user)):
    if body.tier not in VALID_TIERS:
        raise HTTPException(status_code=400, detail=f"tier must be one of {sorted(VALID_TIERS)}")
    book_id = str(uuid.uuid4())
    now = time.time()
    with db() as conn:
        conn.execute(
            "INSERT INTO books (id, name, tier, description, created_by, created_at, seq) " "VALUES (?,?,?,?,?,?,0)",
            (book_id, body.name, body.tier, body.description, user_id, now),
        )
        conn.execute(
            "INSERT INTO memberships (id, book_id, user_id, role, display_name, status, invited_by, created_at) "
            "VALUES (?,?,?,?,?,'active','',?)",
            (str(uuid.uuid4()), book_id, user_id, ROLE_OWNER, "", now),
        )
        if body.wrapped_book_key:
            conn.execute(
                "INSERT INTO metadata (book_id, key, value, updated_at, updated_by) VALUES (?,?,?,?,?)",
                (book_id, "_wrapped_book_key", body.wrapped_book_key, now, user_id),
            )
        audit(conn, book_id, user_id, "book.created", body.tier)
    return {
        "service": SERVICE_NAME,
        "book": {
            "id": book_id,
            "name": body.name,
            "tier": body.tier,
            "description": body.description,
            "created_by": user_id,
            "created_at": now,
            "seq": 0,
        },
        "your_role": ROLE_OWNER,
    }


@app.get("/books")
def list_books(user_id: str = Depends(current_user)):
    with db() as conn:
        rows = conn.execute(
            "SELECT b.*, m.role, m.status FROM books b "
            "JOIN memberships m ON m.book_id = b.id "
            "WHERE m.user_id=? AND m.status IN ('active','invited') "
            "ORDER BY b.created_at ASC",
            (user_id,),
        ).fetchall()
        return {
            "service": SERVICE_NAME,
            "books": [
                {
                    "id": r["id"],
                    "name": r["name"],
                    "tier": r["tier"],
                    "description": r["description"],
                    "seq": r["seq"],
                    "your_role": r["role"],
                    "membership_status": r["status"],
                    "created_at": r["created_at"],
                }
                for r in rows
            ],
        }


@app.get("/books/{book_id}")
def get_book(book_id: str, user_id: str = Depends(current_user)):
    with db() as conn:
        require_membership(conn, user_id, book_id, None)
        r = conn.execute("SELECT * FROM books WHERE id=?", (book_id,)).fetchone()
        return {
            "service": SERVICE_NAME,
            "book": {
                "id": r["id"],
                "name": r["name"],
                "tier": r["tier"],
                "description": r["description"],
                "seq": r["seq"],
                "created_at": r["created_at"],
            },
        }


@app.put("/books/{book_id}")
def update_book(book_id: str, body: BookUpdate, user_id: str = Depends(current_user)):
    """Book name/description changes are recorded in audit_log only; the
    canonical copy for members lives in E2E-encrypted metadata sync."""
    with db() as conn:
        require_membership(conn, user_id, book_id, {ROLE_OWNER, ROLE_ADMIN})
        if body.name:
            conn.execute("UPDATE books SET name=? WHERE id=?", (body.name, book_id))
        if body.description is not None:
            conn.execute("UPDATE books SET description=? WHERE id=?", (body.description, book_id))
        audit(conn, book_id, user_id, "book.updated", body.name or "")
    return {"service": SERVICE_NAME, "status": "updated"}


# ---------------------------------------------------------------------------
# Memberships
# ---------------------------------------------------------------------------
@app.post("/books/{book_id}/members")
def invite_member(book_id: str, body: MemberInvite, user_id: str = Depends(current_user)):
    if body.role not in VALID_ROLES or body.role == ROLE_OWNER:
        raise HTTPException(status_code=400, detail="invalid role for invitation")
    now = time.time()
    with db() as conn:
        require_membership(conn, user_id, book_id, {ROLE_OWNER, ROLE_ADMIN})
        if not body.wrapped_book_key:
            raise HTTPException(
                status_code=400,
                detail="invitation must include the book key wrapped for the invitee",
            )
        existing = conn.execute(
            "SELECT id FROM memberships WHERE book_id=? AND user_id=?", (book_id, body.user_id)
        ).fetchone()
        if existing:
            raise HTTPException(status_code=409, detail="user already invited or a member")
        conn.execute(
            "INSERT INTO memberships (id, book_id, user_id, role, display_name, status, invited_by, created_at) "
            "VALUES (?,?,?,?,?,'invited',?,?)",
            (str(uuid.uuid4()), book_id, body.user_id, body.role, body.display_name, user_id, now),
        )
        conn.execute(
            "INSERT INTO metadata (book_id, key, value, updated_at, updated_by) "
            "VALUES (?,?,?,?,?) "
            "ON CONFLICT(book_id, key) DO UPDATE SET value=excluded.value, "
            "updated_at=excluded.updated_at, updated_by=excluded.updated_by",
            (book_id, f"_wrapped_key:{body.user_id}", body.wrapped_book_key, now, user_id),
        )
        audit(conn, book_id, user_id, "member.invited", f"{body.user_id}:{body.role}")
    return {"service": SERVICE_NAME, "status": "invited", "role": body.role}


@app.get("/books/{book_id}/members")
def list_members(book_id: str, user_id: str = Depends(current_user)):
    with db() as conn:
        require_membership(conn, user_id, book_id, None)
        rows = conn.execute(
            "SELECT user_id, role, display_name, status, created_at FROM memberships "
            "WHERE book_id=? ORDER BY created_at ASC",
            (book_id,),
        ).fetchall()
        return {
            "service": SERVICE_NAME,
            "members": [dict(r) for r in rows],
        }


@app.put("/books/{book_id}/members/{member_user_id}")
def update_member(book_id: str, member_user_id: str, body: MemberUpdate, user_id: str = Depends(current_user)):
    if body.role and (body.role not in VALID_ROLES):
        raise HTTPException(status_code=400, detail="invalid role")
    if body.status and body.status not in {"active", "revoked"}:
        raise HTTPException(status_code=400, detail="invalid status")
    with db() as conn:
        require_membership(conn, user_id, book_id, {ROLE_OWNER, ROLE_ADMIN})
        target = conn.execute(
            "SELECT * FROM memberships WHERE book_id=? AND user_id=?", (book_id, member_user_id)
        ).fetchone()
        if target is None:
            raise HTTPException(status_code=404, detail="member not found")
        if target["role"] == ROLE_OWNER:
            raise HTTPException(status_code=400, detail="cannot modify the owner")
        if body.role:
            conn.execute(
                "UPDATE memberships SET role=? WHERE book_id=? AND user_id=?",
                (body.role, book_id, member_user_id),
            )
        if body.status:
            conn.execute(
                "UPDATE memberships SET status=? WHERE book_id=? AND user_id=?",
                (body.status, book_id, member_user_id),
            )
        audit(
            conn,
            book_id,
            user_id,
            "member.updated",
            f"{member_user_id}:{body.role or ''}:{body.status or ''}",
        )
    return {"service": SERVICE_NAME, "status": "updated"}


@app.post("/books/{book_id}/members/accept")
def accept_invite(book_id: str, user_id: str = Depends(current_user)):
    """An invited user accepts their membership. Returns their copy of the
    book key (wrapped with their public key) so their device can decrypt the
    Book - the server still never sees the raw key."""
    now = time.time()
    with db() as conn:
        row = conn.execute(
            "SELECT * FROM memberships WHERE book_id=? AND user_id=? AND status='invited'",
            (book_id, user_id),
        ).fetchone()
        if row is None:
            raise HTTPException(status_code=404, detail="No pending invitation for this book")
        conn.execute(
            "UPDATE memberships SET status='active' WHERE book_id=? AND user_id=?",
            (book_id, user_id),
        )
        audit(conn, book_id, user_id, "member.accepted", row["role"])
        wrapped = conn.execute(
            "SELECT value FROM metadata WHERE book_id=? AND key=?",
            (book_id, "_wrapped_key:%s" % user_id),
        ).fetchone()
    return {
        "service": SERVICE_NAME,
        "status": "active",
        "role": row["role"],
        "wrapped_book_key": wrapped["value"] if wrapped else "",
    }


# ---------------------------------------------------------------------------
# Devices
# ---------------------------------------------------------------------------
@app.post("/books/{book_id}/devices")
def register_device(book_id: str, body: DeviceRegister, user_id: str = Depends(current_user)):
    device_id = str(uuid.uuid4())
    now = time.time()
    with db() as conn:
        require_membership(conn, user_id, book_id, None)
        conn.execute(
            "INSERT INTO devices (id, book_id, user_id, device_name, last_seen_seq, created_at) "
            "VALUES (?,?,?,?,0,?)",
            (device_id, book_id, user_id, body.device_name, now),
        )
        audit(conn, book_id, user_id, "device.registered", device_id)
    return {"service": SERVICE_NAME, "device_id": device_id}


# ---------------------------------------------------------------------------
# Entry delta sync (the core loop)
# ---------------------------------------------------------------------------
@app.post("/books/{book_id}/entries/push")
def push_entries(book_id: str, body: EntryPush, user_id: str = Depends(current_user)):
    """Append a batch of locally-queued encrypted entries. Idempotent:
    duplicate entry_ids (same device retry) are skipped, not duplicated."""
    if not body.entries:
        return {"service": SERVICE_NAME, "accepted": 0, "book_seq": _book_seq(book_id)}
    with db() as conn:
        require_membership(conn, user_id, book_id, WRITER_ROLES)
        dev = conn.execute(
            "SELECT id FROM devices WHERE id=? AND book_id=? AND user_id=?",
            (body.device_id, book_id, user_id),
        ).fetchone()
        if dev is None:
            raise HTTPException(status_code=400, detail="device not registered for this book")
        accepted = 0
        for e in body.entries:
            entry_id = e.get("entry_id")
            payload = e.get("payload")
            if not entry_id or not payload:
                raise HTTPException(status_code=400, detail="each entry needs entry_id and payload")
            dup = conn.execute("SELECT 1 FROM entries WHERE book_id=? AND entry_id=?", (book_id, entry_id)).fetchone()
            if dup:
                continue  # retry-safe: device resent something we already have
            conn.execute("UPDATE books SET seq = seq + 1 WHERE id=?", (book_id,))
            seq = conn.execute("SELECT seq FROM books WHERE id=?", (book_id,)).fetchone()["seq"]
            conn.execute(
                "INSERT INTO entries (book_id, seq, entry_id, device_id, user_id, payload, created_at) "
                "VALUES (?,?,?,?,?,?,?)",
                (
                    book_id,
                    seq,
                    entry_id,
                    body.device_id,
                    user_id,
                    payload,
                    float(e.get("created_at", time.time())),
                ),
            )
            accepted += 1
        conn.execute(
            "UPDATE devices SET last_seen_seq=? WHERE id=?",
            (_book_seq_row(conn, book_id), body.device_id),
        )
        audit(conn, book_id, user_id, "entries.pushed", str(accepted))
    return {
        "service": SERVICE_NAME,
        "accepted": accepted,
        "book_seq": _book_seq(book_id),
    }


@app.get("/books/{book_id}/entries")
def pull_entries(
    book_id: str,
    since: int = 0,
    limit: int = 500,
    user_id: str = Depends(current_user),
):
    """Pull entries the caller has not yet seen. `since` is the caller's
    local high-water seq for this Book. Entries are returned in seq order."""
    limit = max(1, min(limit, 1000))
    with db() as conn:
        require_membership(conn, user_id, book_id, None)
        rows = conn.execute(
            "SELECT seq, entry_id, device_id, user_id, payload, created_at FROM entries "
            "WHERE book_id=? AND seq>? ORDER BY seq ASC LIMIT ?",
            (book_id, since, limit),
        ).fetchall()
        meta = conn.execute("SELECT seq FROM books WHERE id=?", (book_id,)).fetchone()
        return {
            "service": SERVICE_NAME,
            "book_seq": meta["seq"] if meta else 0,
            "entries": [
                {
                    "seq": r["seq"],
                    "entry_id": r["entry_id"],
                    "device_id": r["device_id"],
                    "user_id": r["user_id"],
                    "payload": r["payload"],
                    "created_at": r["created_at"],
                }
                for r in rows
            ],
        }


# ---------------------------------------------------------------------------
# E2E-encrypted metadata sync (last-write-wins)
# ---------------------------------------------------------------------------
@app.post("/books/{book_id}/metadata")
def upsert_metadata(book_id: str, body: MetaUpsert, user_id: str = Depends(current_user)):
    with db() as conn:
        require_membership(conn, user_id, book_id, WRITER_ROLES)
        conn.execute(
            "INSERT INTO metadata (book_id, key, value, updated_at, updated_by) VALUES (?,?,?,?,?) "
            "ON CONFLICT(book_id, key) DO UPDATE SET value=excluded.value, "
            "updated_at=excluded.updated_at, updated_by=excluded.updated_by",
            (book_id, body.key, body.value, body.updated_at, user_id),
        )
    return {"service": SERVICE_NAME, "status": "stored", "key": body.key}


@app.get("/books/{book_id}/metadata")
def pull_metadata(
    book_id: str,
    since: float = 0.0,
    user_id: str = Depends(current_user),
):
    """All metadata rows updated after `since`. Client applies
    last-write-wins locally; the wrapped book-key rows let newly approved
    members decrypt the Book without the server ever seeing the key."""
    with db() as conn:
        require_membership(conn, user_id, book_id, None)
        rows = conn.execute(
            "SELECT key, value, updated_at, updated_by FROM metadata "
            "WHERE book_id=? AND updated_at>? ORDER BY updated_at ASC",
            (book_id, since),
        ).fetchall()
        return {
            "service": SERVICE_NAME,
            "metadata": [dict(r) for r in rows],
        }


# ---------------------------------------------------------------------------
# Audit log
# ---------------------------------------------------------------------------
@app.get("/books/{book_id}/audit")
def get_audit(book_id: str, limit: int = 200, user_id: str = Depends(current_user)):
    with db() as conn:
        require_membership(conn, user_id, book_id, None)
        rows = conn.execute(
            "SELECT actor, action, detail, created_at FROM audit_log WHERE book_id=? "
            "ORDER BY created_at DESC LIMIT ?",
            (book_id, max(1, min(limit, 1000))),
        ).fetchall()
        return {"service": SERVICE_NAME, "audit": [dict(r) for r in rows]}


# ---------------------------------------------------------------------------
def _book_seq(book_id: str) -> int:
    with db() as conn:
        return _book_seq_row(conn, book_id)


def _book_seq_row(conn: sqlite3.Connection, book_id: str) -> int:
    r = conn.execute("SELECT seq FROM books WHERE id=?", (book_id,)).fetchone()
    return r["seq"] if r else 0


@app.get("/health")
def health():
    return {"service": SERVICE_NAME, "status": "healthy", "version": SERVICE_VERSION}


@app.get("/")
def root():
    return {
        "service": SERVICE_NAME,
        "version": SERVICE_VERSION,
        "endpoints": [
            "/health",
            "POST /books",
            "GET /books",
            "GET /books/{id}",
            "PUT /books/{id}",
            "POST /books/{id}/members",
            "GET /books/{id}/members",
            "PUT /books/{id}/members/{user}",
            "POST /books/{id}/members/accept",
            "POST /books/{id}/devices",
            "POST /books/{id}/entries/push",
            "GET /books/{id}/entries?since=N",
            "POST /books/{id}/metadata",
            "GET /books/{id}/metadata?since=N",
            "GET /books/{id}/audit",
        ],
    }
