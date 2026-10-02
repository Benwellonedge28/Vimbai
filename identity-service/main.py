import os

"""
Vimbai Identity Service
Comprehensive Authentication, Authorization (RBAC), and Session Management
Implements OAuth2/OIDC, JWT, MFA, and Capability-Based Security
"""

import hashlib
import json
import secrets
import uuid
from datetime import datetime, timedelta, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import jwt
import structlog
from dotenv import load_dotenv
from fastapi import Depends, FastAPI, Form, Header, HTTPException, Request, status
from fastapi.security import OAuth2PasswordRequestForm
from passlib.context import CryptContext
from pydantic import BaseModel, EmailStr, Field

load_dotenv()

# Self-bootstrap the package alias so `identity_service.database` resolves
# both under `uvicorn main:app` (service dir on sys.path) and when this file
# is imported as part of the repository test suite.
import importlib.util
import sys as _sys

if "identity_service" not in _sys.modules:
    _spec = importlib.util.spec_from_file_location(
        "identity_service", os.path.join(os.path.dirname(os.path.abspath(__file__)), "__init__.py")
    )
    _mod = importlib.util.module_from_spec(_spec)
    _sys.modules["identity_service"] = _mod
    _spec.loader.exec_module(_mod)
logger = structlog.get_logger()

app = FastAPI(
    title="Vimbai Identity Service",
    description="Authentication, Authorization (RBAC), and Session Management with OAuth2/OIDC, JWT, and MFA support",
    version="1.0.0",
)


# Distributed tracing
try:
    from shared.tracing import get_tracer, setup_tracing

    TRACER = setup_tracing(service_name="identity-service", instrument_app=app)
except ImportError:
    TRACER = None
    import logging

    logging.getLogger(__name__).warning("OpenTelemetry not installed - tracing disabled")

# ============================================================================
# Configuration
# ============================================================================

# os.environ["JWT_SECRET"] is read from the environment at call time, not import time
# (lets tests override it and keeps import side-effect free)
JWT_ALGORITHM = "HS256"
ACCESS_TOKEN_EXPIRE_MINUTES = 60
REFRESH_TOKEN_EXPIRE_DAYS = 7
MFA_CODE_EXPIRE_MINUTES = 5

# ============================================================================
# Enums
# ============================================================================


class UserStatus(str, Enum):
    ACTIVE = "active"
    INACTIVE = "inactive"
    SUSPENDED = "suspended"
    PENDING_VERIFICATION = "pending_verification"


class MFAMethod(str, Enum):
    TOTP = "totp"  # Time-based One-Time Password
    SMS = "sms"
    EMAIL = "email"


class TokenType(str, Enum):
    ACCESS = "access"
    REFRESH = "refresh"
    MFA = "mfa"


class Capability(str, Enum):
    # Accounting Capabilities
    ACCOUNT_VIEW = "account:view"
    ACCOUNT_CREATE = "account:create"
    ACCOUNT_EDIT = "account:edit"
    ACCOUNT_DELETE = "account:delete"
    JOURNAL_VIEW = "journal:view"
    JOURNAL_CREATE = "journal:create"
    JOURNAL_POST = "journal:post"
    JOURNAL_DELETE = "journal:delete"
    # Finance Capabilities
    BUDGET_VIEW = "budget:view"
    BUDGET_CREATE = "budget:create"
    BUDGET_APPROVE = "budget:approve"
    # Reporting Capabilities
    REPORT_VIEW = "report:view"
    REPORT_CREATE = "report:create"
    REPORT_EXPORT = "report:export"
    # Admin Capabilities
    USER_MANAGE = "user:manage"
    ROLE_MANAGE = "role:manage"
    AUDIT_VIEW = "audit:view"
    SYSTEM_CONFIG = "system:config"
    FEATURE_TOGGLE = "feature:toggle"
    # Integration Capabilities
    INTEGRATION_VIEW = "integration:view"
    INTEGRATION_CONFIG = "integration:config"
    INTEGRATION_DELETE = "integration:delete"


# ============================================================================
# Pydantic Models
# ============================================================================


class UserBase(BaseModel):
    email: EmailStr
    username: str = Field(..., min_length=3, max_length=50)
    first_name: Optional[str] = None
    last_name: Optional[str] = None
    phone: Optional[str] = None
    is_active: bool = True


class UserCreate(UserBase):
    password: str = Field(..., min_length=8)
    role_ids: List[str] = []
    organization_id: Optional[str] = None


class UserUpdate(BaseModel):
    email: Optional[EmailStr] = None
    first_name: Optional[str] = None
    last_name: Optional[str] = None
    phone: Optional[str] = None
    is_active: Optional[bool] = None
    role_ids: Optional[List[str]] = None


class User(UserBase):
    id: str
    status: UserStatus
    role_ids: List[str]
    organization_id: Optional[str]
    permissions: List[str] = []
    mfa_enabled: bool = False
    mfa_method: Optional[MFAMethod] = None
    password_hash: str
    created_at: datetime
    updated_at: datetime
    last_login: Optional[datetime] = None


class UserInDB(User):
    password_hash: str


class RoleBase(BaseModel):
    name: str
    description: Optional[str] = None


class RoleCreate(RoleBase):
    permissions: List[str] = []
    is_system: bool = False


class Role(RoleBase):
    id: str
    permissions: List[str]
    is_system: bool
    created_at: datetime
    updated_at: datetime


class OrganizationBase(BaseModel):
    name: str
    description: Optional[str] = None
    settings: Dict[str, Any] = {}


class OrganizationCreate(OrganizationBase):
    admin_email: EmailStr


class Organization(OrganizationBase):
    id: str
    created_at: datetime
    updated_at: datetime


class Token(BaseModel):
    access_token: str
    refresh_token: str
    token_type: str = "bearer"
    expires_in: int = ACCESS_TOKEN_EXPIRE_MINUTES * 60


class TokenPayload(BaseModel):
    sub: str  # user_id
    type: TokenType
    exp: datetime
    iat: datetime
    permissions: List[str] = []
    role: str = ""
    organization_id: Optional[str] = None


class MFASetup(BaseModel):
    method: MFAMethod
    secret: Optional[str] = None
    phone: Optional[str] = None


class MFAVerify(BaseModel):
    code: str = Field(..., min_length=6, max_length=8)
    method: MFAMethod
    temp_token: Optional[str] = None


class PasswordChange(BaseModel):
    current_password: str
    new_password: str = Field(..., min_length=8)


class PasswordReset(BaseModel):
    email: EmailStr


class PasswordResetConfirm(BaseModel):
    reset_token: str
    new_password: str = Field(..., min_length=8)


class AuditLogEntry(BaseModel):
    id: str
    user_id: str
    action: str
    resource_type: Optional[str] = None
    resource_id: Optional[str] = None
    ip_address: Optional[str] = None
    user_agent: Optional[str] = None
    details: Optional[Dict[str, Any]] = None
    timestamp: datetime


# ============================================================================

# ============================================================================
# Storage
# ============================================================================

# Durable identity records (users, roles, organizations, audit logs) persist
# in Neo4j so they survive restarts. Short-lived auth material stays in
# memory by design: sessions, 5-minute MFA challenge codes, 7-day refresh
# tokens, and 24-hour password-reset tokens. Losing those only forces a
# re-login, which is the safe failure mode for an identity service.

sessions: Dict[str, Dict[str, Any]] = {}
mfa_codes: Dict[str, Dict[str, Any]] = {}
password_reset_tokens: Dict[str, Dict[str, Any]] = {}
refresh_tokens: Dict[str, Dict[str, Any]] = {}

_roles_seeded = False
# ============================================================================
# Helper Functions
# ============================================================================

pwd_context = CryptContext(schemes=["bcrypt"], deprecated="auto")


def hash_password(password: str) -> str:
    """Hash a password using bcrypt"""
    return pwd_context.hash(password)


def verify_password(plain_password: str, hashed_password: str) -> bool:
    """Verify a password against its hash"""
    return pwd_context.verify(plain_password, hashed_password)


async def create_access_token(user: User, expires_delta: Optional[timedelta] = None) -> str:
    """Create a JWT access token"""
    if expires_delta:
        expire = datetime.now(timezone.utc) + expires_delta
    else:
        expire = datetime.now(timezone.utc) + timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)

    permissions = await get_user_permissions(user)
    payload = {
        "sub": user.id,
        "type": TokenType.ACCESS.value,
        "exp": expire,
        "iat": datetime.now(timezone.utc),
        "permissions": permissions,
        "role": user.role_ids[0] if user.role_ids else "user",
        "organization_id": user.organization_id,
    }
    return jwt.encode(payload, os.environ["JWT_SECRET"], algorithm=JWT_ALGORITHM)


def create_refresh_token(user: User) -> str:
    """Create a JWT refresh token"""
    token = secrets.token_urlsafe(64)
    expire = datetime.now(timezone.utc) + timedelta(days=REFRESH_TOKEN_EXPIRE_DAYS)

    refresh_tokens[token] = {"user_id": user.id, "exp": expire, "created_at": datetime.now(timezone.utc)}
    return token


def create_mfa_code(user_id: str, method: MFAMethod) -> str:
    """Generate and store MFA code"""
    code = "".join([str(secrets.randbelow(10)) for _ in range(6)])
    expire = datetime.now(timezone.utc) + timedelta(minutes=MFA_CODE_EXPIRE_MINUTES)

    mfa_codes[code] = {"user_id": user_id, "method": method, "exp": expire, "attempts": 0}
    return code


def verify_mfa_code(code: str, user_id: str) -> bool:
    """Verify MFA code"""
    if code not in mfa_codes:
        return False

    mfa_data = mfa_codes[code]
    if mfa_data["user_id"] != user_id:
        return False

    if datetime.now(timezone.utc) > mfa_data["exp"]:
        del mfa_codes[code]
        return False

    del mfa_codes[code]
    return True


async def get_user_permissions(user: User) -> List[str]:
    """Get all permissions for a user based on their (persisted) roles"""
    permissions = set()

    for role_id in user.role_ids:
        role = await db_get_role(role_id)
        if role:
            permissions.update(role.permissions)

    return list(permissions)


# ============================================================================
# Audit Logging (persisted, immutable)
# ============================================================================


async def create_audit_log(
    user_id: str,
    action: str,
    resource_type: Optional[str] = None,
    resource_id: Optional[str] = None,
    request: Request = None,
    details: Dict = None,
):
    """Create an immutable audit log entry (never mutated; corrections append new entries)"""
    log_entry = AuditLogEntry(
        id=str(uuid.uuid4()),
        user_id=user_id,
        action=action,
        resource_type=resource_type,
        resource_id=resource_id,
        ip_address=request.client.host if (request and request.client) else None,
        user_agent=request.headers.get("user-agent") if request else None,
        details=details,
        timestamp=datetime.now(timezone.utc),
    )
    async with _session() as session:
        await session.run(
            """
            CREATE (l:IdentityAuditLog {
                id: $id,
                user_id: $user_id,
                action: $action,
                resource_type: $resource_type,
                resource_id: $resource_id,
                ip_address: $ip_address,
                user_agent: $user_agent,
                details_json: $details_json,
                timestamp: datetime($timestamp)
            })
            """,
            {
                "id": log_entry.id,
                "user_id": log_entry.user_id,
                "action": log_entry.action,
                "resource_type": log_entry.resource_type,
                "resource_id": log_entry.resource_id,
                "ip_address": log_entry.ip_address,
                "user_agent": log_entry.user_agent,
                "details_json": json.dumps(details) if details else None,
                "timestamp": log_entry.timestamp.isoformat(),
            },
        )
    return log_entry


# ============================================================================
# Neo4j Data Access (durable identity records)
# ============================================================================


def _session():
    """Yield a Neo4j session from the shared driver singleton."""
    from identity_service.database import Neo4jConnector

    return Neo4jConnector.get_driver().session()


def _coerce_dt(value):
    """Hydrate a datetime from raw props (fake harness wraps them in Temporal)."""
    if value is None or isinstance(value, datetime):
        return value
    if hasattr(value, "iso_format"):
        return datetime.fromisoformat(value.iso_format())
    if isinstance(value, str):
        return datetime.fromisoformat(value)
    raise TypeError(f"Unrecognised datetime value: {value!r}")


def _user_props(user: User) -> Dict[str, Any]:
    return {
        "id": user.id,
        "email": user.email,
        "username": user.username,
        "first_name": user.first_name,
        "last_name": user.last_name,
        "phone": user.phone,
        "is_active": user.is_active,
        "status": user.status.value if isinstance(user.status, UserStatus) else user.status,
        "role_ids_json": json.dumps(user.role_ids),
        "organization_id": user.organization_id,
        "permissions_json": json.dumps(user.permissions),
        "mfa_enabled": user.mfa_enabled,
        "mfa_method": user.mfa_method.value if isinstance(user.mfa_method, MFAMethod) else user.mfa_method,
        "password_hash": user.password_hash,
        "created_at": user.created_at.isoformat(),
        "updated_at": user.updated_at.isoformat(),
        "last_login": user.last_login.isoformat() if user.last_login else None,
    }


def _user_from_props(p: Dict[str, Any]) -> Optional[User]:
    if not p:
        return None
    return User(
        id=p["id"],
        email=p["email"],
        username=p["username"],
        first_name=p.get("first_name"),
        last_name=p.get("last_name"),
        phone=p.get("phone"),
        is_active=p.get("is_active", True),
        status=UserStatus(p.get("status", "active")),
        role_ids=json.loads(p.get("role_ids_json") or "[]"),
        organization_id=p.get("organization_id"),
        permissions=json.loads(p.get("permissions_json") or "[]"),
        mfa_enabled=p.get("mfa_enabled", False),
        mfa_method=MFAMethod(p["mfa_method"]) if p.get("mfa_method") else None,
        password_hash=p["password_hash"],
        created_at=_coerce_dt(p.get("created_at")),
        updated_at=_coerce_dt(p.get("updated_at")),
        last_login=_coerce_dt(p.get("last_login")),
    )


async def db_create_user(user: User) -> None:
    props = _user_props(user)
    async with _session() as session:
        await session.run(
            """
            CREATE (u:IdentityUser {
                id: $id,
                email: $email,
                username: $username,
                first_name: $first_name,
                last_name: $last_name,
                phone: $phone,
                is_active: $is_active,
                status: $status,
                role_ids_json: $role_ids_json,
                organization_id: $organization_id,
                permissions_json: $permissions_json,
                mfa_enabled: $mfa_enabled,
                mfa_method: $mfa_method,
                password_hash: $password_hash,
                created_at: datetime($created_at),
                updated_at: datetime($updated_at),
                last_login: datetime($last_login)
            })
            """,
            props,
        )


async def db_get_user(user_id: str) -> Optional[User]:
    async with _session() as session:
        result = await session.run("MATCH (u:IdentityUser)\nWHERE u.id = $id\nRETURN u", {"id": user_id})
        record = await result.single()
    if not record:
        return None
    return _user_from_props(record["u"])


async def db_get_user_by_email(email: str) -> Optional[User]:
    async with _session() as session:
        result = await session.run("MATCH (u:IdentityUser)\nWHERE u.email = $email\nRETURN u", {"email": email})
        record = await result.single()
    if not record:
        return None
    return _user_from_props(record["u"])


async def db_get_user_by_username(username: str) -> Optional[User]:
    async with _session() as session:
        result = await session.run(
            "MATCH (u:IdentityUser)\nWHERE u.username = $username\nRETURN u", {"username": username}
        )
        record = await result.single()
    if not record:
        return None
    return _user_from_props(record["u"])


async def db_update_user(user: User) -> None:
    """Persist the full mutable state of a user record."""
    props = _user_props(user)
    async with _session() as session:
        await session.run(
            """
            MATCH (u:IdentityUser)
            WHERE u.id = $id
            SET u.email = $email,
                u.username = $username,
                u.first_name = $first_name,
                u.last_name = $last_name,
                u.phone = $phone,
                u.is_active = $is_active,
                u.status = $status,
                u.role_ids_json = $role_ids_json,
                u.organization_id = $organization_id,
                u.permissions_json = $permissions_json,
                u.mfa_enabled = $mfa_enabled,
                u.mfa_method = $mfa_method,
                u.password_hash = $password_hash,
                u.updated_at = datetime($updated_at),
                u.last_login = datetime($last_login)
            """,
            props,
        )


async def db_count_users() -> int:
    async with _session() as session:
        result = await session.run("MATCH (u:IdentityUser)\nRETURN u", {})
        records = [rec async for rec in result]
    return len(records)


# --- Roles ---


def _role_props(role: Role) -> Dict[str, Any]:
    return {
        "id": role.id,
        "name": role.name,
        "description": role.description,
        "permissions_json": json.dumps(role.permissions),
        "is_system": role.is_system,
        "created_at": role.created_at.isoformat(),
        "updated_at": role.updated_at.isoformat(),
    }


def _role_from_props(p: Dict[str, Any]) -> Role:
    return Role(
        id=p["id"],
        name=p["name"],
        description=p.get("description"),
        permissions=json.loads(p.get("permissions_json") or "[]"),
        is_system=p.get("is_system", False),
        created_at=_coerce_dt(p.get("created_at")),
        updated_at=_coerce_dt(p.get("updated_at")),
    )


async def db_list_roles() -> List[Role]:
    async with _session() as session:
        result = await session.run("MATCH (r:IdentityRole)\nRETURN r", {})
        records = [rec["r"] async for rec in result]
    return [_role_from_props(p) for p in records]


async def db_get_role(role_id: str) -> Optional[Role]:
    async with _session() as session:
        result = await session.run("MATCH (r:IdentityRole)\nWHERE r.id = $id\nRETURN r", {"id": role_id})
        record = await result.single()
    if not record:
        return None
    return _role_from_props(record["r"])


async def db_create_role(role: Role) -> None:
    async with _session() as session:
        await session.run(
            """
            CREATE (r:IdentityRole {
                id: $id,
                name: $name,
                description: $description,
                permissions_json: $permissions_json,
                is_system: $is_system,
                created_at: datetime($created_at),
                updated_at: datetime($updated_at)
            })
            """,
            _role_props(role),
        )


async def db_update_role(role: Role) -> None:
    async with _session() as session:
        await session.run(
            """
            MATCH (r:IdentityRole)
            WHERE r.id = $id
            SET r.name = $name,
                r.description = $description,
                r.permissions_json = $permissions_json,
                r.updated_at = datetime($updated_at)
            """,
            _role_props(role),
        )


# --- Organizations ---


async def db_create_org(org: Organization) -> None:
    async with _session() as session:
        await session.run(
            """
            CREATE (o:IdentityOrganization {
                id: $id,
                name: $name,
                description: $description,
                settings_json: $settings_json,
                created_at: datetime($created_at),
                updated_at: datetime($updated_at)
            })
            """,
            {
                "id": org.id,
                "name": org.name,
                "description": org.description,
                "settings_json": json.dumps(org.settings),
                "created_at": org.created_at.isoformat(),
                "updated_at": org.updated_at.isoformat(),
            },
        )


async def db_get_org(org_id: str) -> Optional[Organization]:
    async with _session() as session:
        result = await session.run("MATCH (o:IdentityOrganization)\nWHERE o.id = $id\nRETURN o", {"id": org_id})
        record = await result.single()
    if not record:
        return None
    p = record["o"]
    return Organization(
        id=p["id"],
        name=p["name"],
        description=p.get("description"),
        settings=json.loads(p.get("settings_json") or "{}"),
        created_at=_coerce_dt(p.get("created_at")),
        updated_at=_coerce_dt(p.get("updated_at")),
    )


# --- Audit logs ---


async def db_list_audit_logs() -> List[AuditLogEntry]:
    async with _session() as session:
        result = await session.run("MATCH (l:IdentityAuditLog)\nRETURN l", {})
        records = [rec["l"] async for rec in result]
    entries = []
    for p in records:
        details = json.loads(p["details_json"]) if p.get("details_json") else None
        entries.append(
            AuditLogEntry(
                id=p["id"],
                user_id=p["user_id"],
                action=p["action"],
                resource_type=p.get("resource_type"),
                resource_id=p.get("resource_id"),
                ip_address=p.get("ip_address"),
                user_agent=p.get("user_agent"),
                details=details,
                timestamp=_coerce_dt(p.get("timestamp")),
            )
        )
    return entries


def generate_password_reset_token() -> tuple[str, str]:
    """Generate password reset token (token, hashed_token)"""
    raw_token = secrets.token_urlsafe(64)
    hashed_token = hashlib.sha256(raw_token.encode()).hexdigest()
    return raw_token, hashed_token


# ============================================================================
# Default Roles Setup
# ============================================================================


# ============================================================================
# Default Roles Setup (idempotent; persisted on first use)
# ============================================================================

DEFAULT_ROLE_DEFS = [
    {
        "id": "admin",
        "name": "Administrator",
        "description": "Full system access",
        "permissions": [p.value for p in Capability],
        "is_system": True,
    },
    {
        "id": "accountant",
        "name": "Accountant",
        "description": "Accounting operations access",
        "permissions": [
            Capability.ACCOUNT_VIEW.value,
            Capability.ACCOUNT_CREATE.value,
            Capability.ACCOUNT_EDIT.value,
            Capability.JOURNAL_VIEW.value,
            Capability.JOURNAL_CREATE.value,
            Capability.JOURNAL_POST.value,
            Capability.REPORT_VIEW.value,
            Capability.REPORT_CREATE.value,
            Capability.REPORT_EXPORT.value,
        ],
        "is_system": True,
    },
    {
        "id": "finance_manager",
        "name": "Finance Manager",
        "description": "Financial management access",
        "permissions": [
            Capability.ACCOUNT_VIEW.value,
            Capability.ACCOUNT_CREATE.value,
            Capability.ACCOUNT_EDIT.value,
            Capability.JOURNAL_VIEW.value,
            Capability.JOURNAL_CREATE.value,
            Capability.JOURNAL_POST.value,
            Capability.BUDGET_VIEW.value,
            Capability.BUDGET_CREATE.value,
            Capability.BUDGET_APPROVE.value,
            Capability.REPORT_VIEW.value,
            Capability.REPORT_CREATE.value,
            Capability.REPORT_EXPORT.value,
        ],
        "is_system": True,
    },
    {
        "id": "viewer",
        "name": "Viewer",
        "description": "Read-only access",
        "permissions": [
            Capability.ACCOUNT_VIEW.value,
            Capability.JOURNAL_VIEW.value,
            Capability.REPORT_VIEW.value,
            Capability.REPORT_EXPORT.value,
        ],
        "is_system": True,
    },
    {
        "id": "user",
        "name": "User",
        "description": "Basic user access",
        "permissions": [Capability.ACCOUNT_VIEW.value, Capability.JOURNAL_VIEW.value, Capability.REPORT_VIEW.value],
        "is_system": True,
    },
]


async def ensure_default_roles():
    """Seed the system roles once per process; MERGE-style idempotency via existence check."""
    global _roles_seeded
    if _roles_seeded:
        return
    existing = {role.id for role in await db_list_roles()}
    now = datetime.now(timezone.utc)
    for role_def in DEFAULT_ROLE_DEFS:
        if role_def["id"] in existing:
            continue
        await db_create_role(
            Role(
                id=role_def["id"],
                name=role_def["name"],
                description=role_def["description"],
                permissions=role_def["permissions"],
                is_system=role_def["is_system"],
                created_at=now,
                updated_at=now,
            )
        )
    _roles_seeded = True


@app.on_event("startup")
async def seed_roles_on_startup():
    await ensure_default_roles()


# ============================================================================
# Authentication Dependency (JWT bearer -> caller identity)
# ============================================================================


async def authenticate(request: Request, authorization: str = Header(None)) -> Dict[str, Any]:
    """Resolve the caller from their Bearer JWT. Returns the caller context.

    Raises 401 for missing/invalid/expired tokens or unknown users, 403 for
    accounts that are not active.
    """
    if not authorization:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Not authenticated")

    try:
        scheme, token = authorization.split()
        if scheme.lower() != "bearer":
            raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid authentication scheme")

        payload = jwt.decode(token, os.environ["JWT_SECRET"], algorithms=[JWT_ALGORITHM])
        user_id = payload["sub"]
    except (ValueError, jwt.PyJWTError):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token")

    user = await db_get_user(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token")

    if user.status != UserStatus.ACTIVE:
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Account is not active")

    return {"id": user.id, "user": user, "permissions": await get_user_permissions(user)}


def require_permission(current_user: Dict[str, Any], capability: Capability):
    """Ensure the authenticated caller holds the given capability."""
    if capability.value not in current_user.get("permissions", []):
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail=f"Missing capability: {capability.value}")
    return current_user


def require_self_or_admin(caller: Dict[str, Any], user_id: str):
    """User records are private: only the user themselves or a USER_MANAGE admin.

    Foreign ids return 404 so existence is not disclosed to other callers.
    """
    if caller["id"] != user_id and Capability.USER_MANAGE.value not in caller.get("permissions", []):
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")
    return caller


# ============================================================================
# API Endpoints
# ============================================================================


@app.get("/")
async def root():
    """Health check"""
    return {
        "service": "identity-service",
        "status": "running",
        "version": "1.0.0",
        "total_users": await db_count_users(),
    }


@app.post("/users/register", status_code=status.HTTP_201_CREATED)
async def register_user(user_data: UserCreate, request: Request):
    """Register a new user"""
    await ensure_default_roles()

    if await db_get_user_by_email(user_data.email):
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Email already registered")

    if await db_get_user_by_username(user_data.username):
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Username already taken")

    # Validate role IDs
    for role_id in user_data.role_ids:
        if not await db_get_role(role_id):
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=f"Role {role_id} not found")

    now = datetime.now(timezone.utc)
    user_id = str(uuid.uuid4())

    user = User(
        id=user_id,
        email=user_data.email,
        username=user_data.username,
        first_name=user_data.first_name,
        last_name=user_data.last_name,
        phone=user_data.phone,
        # No email-verification flow exists in this service; self-registered
        # accounts must be usable immediately or login is permanently broken.
        status=UserStatus.ACTIVE,
        role_ids=user_data.role_ids,
        organization_id=user_data.organization_id,
        permissions=[],
        mfa_enabled=False,
        password_hash=hash_password(user_data.password),
        created_at=now,
        updated_at=now,
    )

    await db_create_user(user)
    await create_audit_log(user_id, "user.registered", "User", user_id, request)

    return {"id": user_id, "email": user.email, "username": user.username}


@app.post("/users/login")
async def login(request: Request, form_data: OAuth2PasswordRequestForm = Depends()):
    """User login with username/email and password"""
    # Find user by username or email
    user = await db_get_user_by_username(form_data.username) or await db_get_user_by_email(form_data.username)

    if not user:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid credentials",
            headers={"WWW-Authenticate": "Bearer"},
        )

    if user.status != UserStatus.ACTIVE:
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Account is not active")

    if not verify_password(form_data.password, user.password_hash):
        await create_audit_log(user.id, "login.failed", "User", user.id, request, {"reason": "invalid_password"})
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid credentials",
            headers={"WWW-Authenticate": "Bearer"},
        )

    # Check if MFA is enabled
    if user.mfa_enabled:
        mfa_code = create_mfa_code(user.id, user.mfa_method)
        # In production, send MFA code via configured method
        return {
            "mfa_required": True,
            "method": user.mfa_method.value,
            "temp_token": await create_access_token(user, expires_delta=timedelta(minutes=5)),
        }

    # Create session
    session_id = str(uuid.uuid4())
    sessions[session_id] = {
        "user_id": user.id,
        "created_at": datetime.now(timezone.utc),
        "last_activity": datetime.now(timezone.utc),
        "ip_address": request.client.host if request.client else None,
    }

    # Update last login
    user.last_login = datetime.now(timezone.utc)
    user.updated_at = datetime.now(timezone.utc)
    await db_update_user(user)

    await create_audit_log(user.id, "login.success", "User", user.id, request)

    return Token(
        access_token=await create_access_token(user),
        refresh_token=create_refresh_token(user),
        expires_in=ACCESS_TOKEN_EXPIRE_MINUTES * 60,
    )


@app.post("/users/login/verify-mfa")
async def verify_mfa_login(mfa_data: MFAVerify, request: Request):
    """Verify MFA code and complete login"""
    try:
        payload = jwt.decode(mfa_data.temp_token, os.environ["JWT_SECRET"], algorithms=[JWT_ALGORITHM])
        user_id = payload["sub"]
    except jwt.PyJWTError:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid temporary token")

    user = await db_get_user(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    if not verify_mfa_code(mfa_data.code, user_id):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid MFA code")

    session_id = str(uuid.uuid4())
    sessions[session_id] = {
        "user_id": user_id,
        "created_at": datetime.now(timezone.utc),
        "last_activity": datetime.now(timezone.utc),
        "ip_address": request.client.host if request.client else None,
    }

    user.last_login = datetime.now(timezone.utc)
    user.updated_at = datetime.now(timezone.utc)
    await db_update_user(user)

    await create_audit_log(user_id, "mfa.verified", "User", user_id, request)

    return Token(
        access_token=await create_access_token(user),
        refresh_token=create_refresh_token(user),
        expires_in=ACCESS_TOKEN_EXPIRE_MINUTES * 60,
    )


@app.get("/users/me")
async def get_current_user(caller: Dict[str, Any] = Depends(authenticate)):
    """Get current authenticated user"""
    user = caller["user"]
    return {
        "id": user.id,
        "email": user.email,
        "username": user.username,
        "first_name": user.first_name,
        "last_name": user.last_name,
        "role_ids": user.role_ids,
        "permissions": await get_user_permissions(user),
        "organization_id": user.organization_id,
        "mfa_enabled": user.mfa_enabled,
        "last_login": user.last_login,
    }


@app.get("/users/{user_id}")
async def get_user(user_id: str, caller: Dict[str, Any] = Depends(authenticate)):
    """Get user by ID (self or USER_MANAGE admin only)"""
    require_self_or_admin(caller, user_id)

    user = await db_get_user(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    return {
        "id": user.id,
        "email": user.email,
        "username": user.username,
        "first_name": user.first_name,
        "last_name": user.last_name,
        "status": user.status.value,
        "role_ids": user.role_ids,
        "mfa_enabled": user.mfa_enabled,
        "created_at": user.created_at,
        "last_login": user.last_login,
    }


@app.put("/users/{user_id}")
async def update_user(
    user_id: str,
    update_data: UserUpdate,
    request: Request,
    caller: Dict[str, Any] = Depends(authenticate),
):
    """Update user details (self or USER_MANAGE admin only)"""
    require_self_or_admin(caller, user_id)

    user = await db_get_user(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    if update_data.email:
        existing = await db_get_user_by_email(update_data.email)
        if existing and existing.id != user_id:
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Email already in use")
        user.email = update_data.email

    if update_data.first_name is not None:
        user.first_name = update_data.first_name
    if update_data.last_name is not None:
        user.last_name = update_data.last_name
    if update_data.phone is not None:
        user.phone = update_data.phone
    if update_data.is_active is not None:
        user.is_active = update_data.is_active
        user.status = UserStatus.ACTIVE if update_data.is_active else UserStatus.INACTIVE
    if update_data.role_ids is not None:
        await ensure_default_roles()
        for role_id in update_data.role_ids:
            if not await db_get_role(role_id):
                raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=f"Role {role_id} not found")
        user.role_ids = update_data.role_ids

    user.updated_at = datetime.now(timezone.utc)
    await db_update_user(user)

    await create_audit_log(
        user_id, "user.updated", "User", user_id, request, {"updated_fields": update_data.model_dump(exclude_none=True)}
    )

    return {"ok": True, "updated_at": user.updated_at}


@app.post("/users/{user_id}/change-password")
async def change_password(
    user_id: str, passwords: PasswordChange, request: Request, caller: Dict[str, Any] = Depends(authenticate)
):
    """Change user password (self only: requires knowledge of the current password)"""
    if caller["id"] != user_id:
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Cannot change another user's password")

    user = await db_get_user(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    if not verify_password(passwords.current_password, user.password_hash):
        await create_audit_log(
            user_id, "password.change.failed", "User", user_id, request, {"reason": "invalid_current_password"}
        )
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Current password is incorrect")

    user.password_hash = hash_password(passwords.new_password)
    user.updated_at = datetime.now(timezone.utc)
    await db_update_user(user)

    # Invalidate all refresh tokens
    for token_id, token_data in list(refresh_tokens.items()):
        if token_data["user_id"] == user_id:
            del refresh_tokens[token_id]

    await create_audit_log(user_id, "password.changed", "User", user_id, request)

    return {"ok": True, "message": "Password changed successfully"}


# --- MFA Management ---


@app.post("/users/{user_id}/mfa/setup")
async def setup_mfa(
    user_id: str, mfa_setup: MFASetup, request: Request, caller: Dict[str, Any] = Depends(authenticate)
):
    """Setup MFA for user (self or USER_MANAGE admin only)"""
    require_self_or_admin(caller, user_id)

    user = await db_get_user(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    # Generate TOTP secret if using TOTP
    secret = None
    if mfa_setup.method == MFAMethod.TOTP:
        secret = secrets.token_urlsafe(32)

    # Send MFA code via configured method
    code = create_mfa_code(user_id, mfa_setup.method)

    # In production, send code via email/SMS

    return {
        "mfa_method": mfa_setup.method.value,
        "secret": secret,
        "code_sent": True,
        "message": f"MFA code sent via {mfa_setup.method.value}",
    }


@app.post("/users/{user_id}/mfa/verify")
async def verify_mfa_setup(
    user_id: str, mfa_verify: MFAVerify, request: Request, caller: Dict[str, Any] = Depends(authenticate)
):
    """Verify MFA setup (self or USER_MANAGE admin only)"""
    require_self_or_admin(caller, user_id)

    user = await db_get_user(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    if not verify_mfa_code(mfa_verify.code, user_id):
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid MFA code")

    user.mfa_enabled = True
    user.mfa_method = mfa_verify.method
    user.updated_at = datetime.now(timezone.utc)
    await db_update_user(user)

    await create_audit_log(user_id, "mfa.enabled", "User", user_id, request)

    return {"ok": True, "message": "MFA enabled successfully"}


@app.post("/users/{user_id}/mfa/disable")
async def disable_mfa(
    user_id: str, code: str = Form(...), request: Request = None, caller: Dict[str, Any] = Depends(authenticate)
):
    """Disable MFA for user (self or USER_MANAGE admin only)"""
    require_self_or_admin(caller, user_id)

    user = await db_get_user(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    if not verify_mfa_code(code, user_id):
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid MFA code")

    user.mfa_enabled = False
    user.mfa_method = None
    user.updated_at = datetime.now(timezone.utc)
    await db_update_user(user)

    await create_audit_log(user_id, "mfa.disabled", "User", user_id, request)

    return {"ok": True, "message": "MFA disabled successfully"}


# --- Role Management ---


@app.get("/roles")
async def list_roles(caller: Dict[str, Any] = Depends(authenticate)):
    """List all available roles (authenticated users only)"""
    await ensure_default_roles()
    roles = await db_list_roles()
    return {"total": len(roles), "roles": roles}


@app.post("/roles", status_code=status.HTTP_201_CREATED)
async def create_role(
    role_data: RoleCreate,
    request: Request = None,
    caller: Dict[str, Any] = Depends(authenticate),
):
    """Create a new role (admin only)"""
    require_permission(caller, Capability.USER_MANAGE)
    role_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    role = Role(
        id=role_id,
        name=role_data.name,
        description=role_data.description,
        permissions=role_data.permissions,
        is_system=role_data.is_system,
        created_at=now,
        updated_at=now,
    )

    await db_create_role(role)

    await create_audit_log(caller["id"], "role.created", "Role", role_id, request, {"name": role_data.name})

    return role


@app.get("/roles/{role_id}")
async def get_role(role_id: str, caller: Dict[str, Any] = Depends(authenticate)):
    """Get role by ID (authenticated users only)"""
    role = await db_get_role(role_id)
    if not role:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Role not found")
    return role


@app.put("/roles/{role_id}")
async def update_role(
    role_id: str,
    update_data: RoleCreate,
    request: Request = None,
    caller: Dict[str, Any] = Depends(authenticate),
):
    """Update a role (admin only)"""
    require_permission(caller, Capability.USER_MANAGE)
    role = await db_get_role(role_id)
    if not role:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Role not found")

    if role.is_system:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Cannot modify system role")

    role.name = update_data.name
    role.description = update_data.description
    role.permissions = update_data.permissions
    role.updated_at = datetime.now(timezone.utc)

    await db_update_role(role)

    await create_audit_log(caller["id"], "role.updated", "Role", role_id, request, {"name": update_data.name})

    return role


# --- Organization Management ---


@app.post("/organizations", status_code=status.HTTP_201_CREATED)
async def create_organization(
    org_data: OrganizationCreate, request: Request = None, caller: Dict[str, Any] = Depends(authenticate)
):
    """Create a new organization (authenticated users only)"""
    org_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    org = Organization(
        id=org_id,
        name=org_data.name,
        description=org_data.description,
        settings=org_data.settings,
        created_at=now,
        updated_at=now,
    )

    await db_create_org(org)

    await create_audit_log(
        caller["id"], "organization.created", "Organization", org_id, request, {"name": org_data.name}
    )

    return org


@app.get("/organizations/{org_id}")
async def get_organization(org_id: str, caller: Dict[str, Any] = Depends(authenticate)):
    """Get organization by ID (members and USER_MANAGE admins only)"""
    org = await db_get_org(org_id)
    if not org:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Organization not found")

    caller_org = caller["user"].organization_id
    is_member = caller_org is not None and caller_org == org_id
    is_admin = Capability.USER_MANAGE.value in caller.get("permissions", [])
    if not (is_member or is_admin):
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Organization not found")

    return org


# --- Token Refresh ---


@app.post("/token/refresh")
async def refresh_token(refresh_token: str = Form(...)):
    """Refresh access token using refresh token"""
    if refresh_token not in refresh_tokens:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid refresh token")

    token_data = refresh_tokens[refresh_token]

    if datetime.now(timezone.utc) > token_data["exp"]:
        del refresh_tokens[refresh_token]
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Refresh token expired")

    user = await db_get_user(token_data["user_id"])
    if not user or user.status != UserStatus.ACTIVE:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="User not active")

    # Generate new tokens
    new_access_token = await create_access_token(user)
    new_refresh_token = create_refresh_token(user)

    # Delete old refresh token
    del refresh_tokens[refresh_token]

    return Token(
        access_token=new_access_token, refresh_token=new_refresh_token, expires_in=ACCESS_TOKEN_EXPIRE_MINUTES * 60
    )


@app.post("/token/revoke")
async def revoke_token(refresh_token: str = Form(...)):
    """Revoke refresh token (logout)"""
    if refresh_token in refresh_tokens:
        del refresh_tokens[refresh_token]
    return {"ok": True}


# --- Password Reset ---


@app.post("/password/reset")
async def request_password_reset(reset_data: PasswordReset, request: Request = None):
    """Request password reset"""
    user = await db_get_user_by_email(reset_data.email)

    if user:
        raw_token, hashed_token = generate_password_reset_token()

        password_reset_tokens[hashed_token] = {
            "user_id": user.id,
            "created_at": datetime.now(timezone.utc),
            "exp": datetime.now(timezone.utc) + timedelta(hours=24),
        }

        # In production, send email with reset link containing raw_token

        await create_audit_log(user.id, "password.reset.requested", "User", user.id, request)

    # Always return success to prevent email enumeration
    return {"message": "If the email exists, a password reset link has been sent"}


@app.post("/password/reset/confirm")
async def confirm_password_reset(confirm_data: PasswordResetConfirm, request: Request = None):
    """Confirm password reset with token"""
    hashed_token = hashlib.sha256(confirm_data.reset_token.encode()).hexdigest()

    if hashed_token not in password_reset_tokens:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid or expired reset token")

    token_data = password_reset_tokens[hashed_token]

    if datetime.now(timezone.utc) > token_data["exp"]:
        del password_reset_tokens[hashed_token]
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Reset token expired")

    user = await db_get_user(token_data["user_id"])
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    user.password_hash = hash_password(confirm_data.new_password)
    user.updated_at = datetime.now(timezone.utc)
    user.status = UserStatus.ACTIVE
    await db_update_user(user)

    del password_reset_tokens[hashed_token]

    # Invalidate all refresh tokens for this user
    for token_id, data in list(refresh_tokens.items()):
        if data["user_id"] == user.id:
            del refresh_tokens[token_id]

    await create_audit_log(user.id, "password.reset.completed", "User", user.id, request)

    return {"ok": True, "message": "Password reset successfully"}


# --- Audit Logs ---


@app.get("/audit-logs")
async def list_audit_logs(
    user_id: Optional[str] = None,
    action: Optional[str] = None,
    resource_type: Optional[str] = None,
    start_date: Optional[datetime] = None,
    end_date: Optional[datetime] = None,
    limit: int = 100,
    offset: int = 0,
    caller: Dict[str, Any] = Depends(authenticate),
):
    """List audit log entries (USER_MANAGE admins only)"""
    require_permission(caller, Capability.USER_MANAGE)

    results = await db_list_audit_logs()

    if user_id:
        results = [log for log in results if log.user_id == user_id]
    if action:
        results = [log for log in results if log.action == action]
    if resource_type:
        results = [log for log in results if log.resource_type == resource_type]
    if start_date:
        results = [log for log in results if log.timestamp >= start_date]
    if end_date:
        results = [log for log in results if log.timestamp <= end_date]

    results.sort(key=lambda x: x.timestamp, reverse=True)
    total = len(results)
    results = results[offset : offset + limit]

    return {"total": total, "logs": results}


# --- Session Management ---


@app.get("/sessions")
async def list_sessions(caller: Dict[str, Any] = Depends(authenticate)):
    """List active sessions for the current caller only"""
    own = [
        {**data, "session_id": session_id} for session_id, data in sessions.items() if data["user_id"] == caller["id"]
    ]
    return {"total": len(own), "sessions": own}


@app.delete("/sessions/{session_id}")
async def revoke_session(session_id: str, caller: Dict[str, Any] = Depends(authenticate)):
    """Revoke a session (own sessions, or any session if USER_MANAGE admin)"""
    is_admin = Capability.USER_MANAGE.value in caller.get("permissions", [])
    if session_id in sessions:
        if sessions[session_id]["user_id"] != caller["id"] and not is_admin:
            raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Session not found")
        del sessions[session_id]
        return {"ok": True}
    raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Session not found")


# --- Capabilities Reference ---


@app.get("/capabilities")
async def list_capabilities():
    """List all available capabilities"""
    return {"capabilities": [{"name": c.value, "description": c.name.replace("_", " ").title()} for c in Capability]}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8080)
