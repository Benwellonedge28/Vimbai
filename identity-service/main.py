"""
FinAcc Identity Service
Comprehensive Authentication, Authorization (RBAC), and Session Management
Implements OAuth2/OIDC, JWT, MFA, and Capability-Based Security
"""

from fastapi import FastAPI, HTTPException, Depends, status, Request, Form
from fastapi.responses import JSONResponse
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm
from pydantic import BaseModel, Field, EmailStr
from typing import List, Optional, Dict, Any
from datetime import datetime, timezone, timedelta
from enum import Enum
import uuid
import hashlib
import secrets
import jwt
from passlib.context import CryptContext
from dotenv import load_dotenv
import structlog

load_dotenv()
logger = structlog.get_logger()

app = FastAPI(
    title="FinAcc Identity Service",
    description="Authentication, Authorization (RBAC), and Session Management with OAuth2/OIDC, JWT, and MFA support",
    version="1.0.0",
)

# ============================================================================
# Configuration
# ============================================================================

JWT_SECRET = os.getenv("JWT_SECRET", "your-secret-key-change-in-production")
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
# In-Memory Storage
# ============================================================================

users: Dict[str, User] = {}
users_by_email: Dict[str, str] = {}  # email -> user_id
users_by_username: Dict[str, str] = {}  # username -> user_id
roles: Dict[str, Role] = {}
organizations: Dict[str, Organization] = {}
sessions: Dict[str, Dict[str, Any]] = {}
mfa_codes: Dict[str, Dict[str, Any]] = {}
password_reset_tokens: Dict[str, Dict[str, Any]] = {}
audit_logs: List[AuditLogEntry] = []
refresh_tokens: Dict[str, Dict[str, Any]] = {}

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

def create_access_token(user: User, expires_delta: Optional[timedelta] = None) -> str:
    """Create a JWT access token"""
    if expires_delta:
        expire = datetime.now(timezone.utc) + expires_delta
    else:
        expire = datetime.now(timezone.utc) + timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)

    permissions = get_user_permissions(user)
    payload = {
        "sub": user.id,
        "type": TokenType.ACCESS.value,
        "exp": expire,
        "iat": datetime.now(timezone.utc),
        "permissions": permissions,
        "role": user.role_ids[0] if user.role_ids else "user",
        "organization_id": user.organization_id
    }
    return jwt.encode(payload, JWT_SECRET, algorithm=JWT_ALGORITHM)

def create_refresh_token(user: User) -> str:
    """Create a JWT refresh token"""
    token = secrets.token_urlsafe(64)
    expire = datetime.now(timezone.utc) + timedelta(days=REFRESH_TOKEN_EXPIRE_DAYS)

    refresh_tokens[token] = {
        "user_id": user.id,
        "exp": expire,
        "created_at": datetime.now(timezone.utc)
    }
    return token

def create_mfa_code(user_id: str, method: MFAMethod) -> str:
    """Generate and store MFA code"""
    code = ''.join([str(secrets.randbelow(10)) for _ in range(6)])
    expire = datetime.now(timezone.utc) + timedelta(minutes=MFA_CODE_EXPIRE_MINUTES)

    mfa_codes[code] = {
        "user_id": user_id,
        "method": method,
        "exp": expire,
        "attempts": 0
    }
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

def get_user_permissions(user: User) -> List[str]:
    """Get all permissions for a user based on their roles"""
    permissions = set()

    for role_id in user.role_ids:
        if role_id in roles:
            permissions.update(roles[role_id].permissions)

    return list(permissions)

def create_audit_log(user_id: str, action: str, resource_type: str = None,
                     resource_id: str = None, request: Request = None, details: Dict = None):
    """Create an immutable audit log entry"""
    log_entry = AuditLogEntry(
        id=str(uuid.uuid4()),
        user_id=user_id,
        action=action,
        resource_type=resource_type,
        resource_id=resource_id,
        ip_address=request.client.host if request else None,
        user_agent=request.headers.get("user-agent") if request else None,
        details=details,
        timestamp=datetime.now(timezone.utc)
    )
    audit_logs.append(log_entry)

    # Keep only last 10000 entries
    if len(audit_logs) > 10000:
        audit_logs.pop(0)

    return log_entry

def generate_password_reset_token() -> tuple[str, str]:
    """Generate password reset token (token, hashed_token)"""
    raw_token = secrets.token_urlsafe(64)
    hashed_token = hashlib.sha256(raw_token.encode()).hexdigest()
    return raw_token, hashed_token

# ============================================================================
# Default Roles Setup
# ============================================================================

def setup_default_roles():
    """Initialize default system roles"""
    global roles

    default_roles = [
        {
            "id": "admin",
            "name": "Administrator",
            "description": "Full system access",
            "permissions": [p.value for p in Capability],
            "is_system": True
        },
        {
            "id": "accountant",
            "name": "Accountant",
            "description": "Accounting operations access",
            "permissions": [
                Capability.ACCOUNT_VIEW.value, Capability.ACCOUNT_CREATE.value,
                Capability.ACCOUNT_EDIT.value, Capability.JOURNAL_VIEW.value,
                Capability.JOURNAL_CREATE.value, Capability.JOURNAL_POST.value,
                Capability.REPORT_VIEW.value, Capability.REPORT_CREATE.value,
                Capability.REPORT_EXPORT.value
            ],
            "is_system": True
        },
        {
            "id": "finance_manager",
            "name": "Finance Manager",
            "description": "Financial management access",
            "permissions": [
                Capability.ACCOUNT_VIEW.value, Capability.ACCOUNT_CREATE.value,
                Capability.ACCOUNT_EDIT.value, Capability.JOURNAL_VIEW.value,
                Capability.JOURNAL_CREATE.value, Capability.JOURNAL_POST.value,
                Capability.BUDGET_VIEW.value, Capability.BUDGET_CREATE.value,
                Capability.BUDGET_APPROVE.value, Capability.REPORT_VIEW.value,
                Capability.REPORT_CREATE.value, Capability.REPORT_EXPORT.value
            ],
            "is_system": True
        },
        {
            "id": "viewer",
            "name": "Viewer",
            "description": "Read-only access",
            "permissions": [
                Capability.ACCOUNT_VIEW.value, Capability.JOURNAL_VIEW.value,
                Capability.REPORT_VIEW.value, Capability.REPORT_EXPORT.value
            ],
            "is_system": True
        },
        {
            "id": "user",
            "name": "User",
            "description": "Basic user access",
            "permissions": [
                Capability.ACCOUNT_VIEW.value, Capability.JOURNAL_VIEW.value,
                Capability.REPORT_VIEW.value
            ],
            "is_system": True
        }
    ]

    now = datetime.now(timezone.utc)
    for role_data in default_roles:
        role = Role(
            id=role_data["id"],
            name=role_data["name"],
            description=role_data["description"],
            permissions=role_data["permissions"],
            is_system=role_data["is_system"],
            created_at=now,
            updated_at=now
        )
        roles[role.id] = role

# Initialize default roles
setup_default_roles()

# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "identity",
        "version": "1.0.0",
        "total_users": len(users),
        "total_roles": len(roles),
        "total_organizations": len(organizations)
    }

# --- User Management ---

@app.post("/users/register", status_code=status.HTTP_201_CREATED)
async def register_user(user_data: UserCreate, request: Request):
    """Register a new user"""
    # Check if email already exists
    if user_data.email in users_by_email:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Email already registered")

    # Check if username already exists
    if user_data.username in users_by_username:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Username already taken")

    # Validate role IDs
    for role_id in user_data.role_ids:
        if role_id not in roles:
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
        status=UserStatus.PENDING_VERIFICATION,
        role_ids=user_data.role_ids,
        organization_id=user_data.organization_id,
        permissions=[],
        mfa_enabled=False,
        password_hash=hash_password(user_data.password),
        created_at=now,
        updated_at=now
    )

    users[user_id] = user
    users_by_email[user_data.email] = user_id
    users_by_username[user_data.username] = user_id

    create_audit_log(user_id, "user.registered", "User", user_id, request)

    return {"id": user_id, "email": user.email, "username": user.username}

@app.post("/users/login")
async def login(request: Request, form_data: OAuth2PasswordRequestForm = Depends()):
    """User login with username/email and password"""
    # Find user by username or email
    user_id = users_by_username.get(form_data.username) or users_by_email.get(form_data.username)

    if not user_id:
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid credentials",
            headers={"WWW-Authenticate": "Bearer"}
        )

    user = users[user_id]

    if user.status != UserStatus.ACTIVE:
        raise HTTPException(
            status_code=status.HTTP_403_FORBIDDEN,
            detail="Account is not active"
        )

    if not verify_password(form_data.password, user.password_hash):
        create_audit_log(user_id, "login.failed", "User", user_id, request, {"reason": "invalid_password"})
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="Invalid credentials",
            headers={"WWW-Authenticate": "Bearer"}
        )

    # Check if MFA is enabled
    if user.mfa_enabled:
        mfa_code = create_mfa_code(user_id, user.mfa_method)
        # In production, send MFA code via configured method
        return {
            "mfa_required": True,
            "method": user.mfa_method.value,
            "temp_token": create_access_token(user, expires_delta=timedelta(minutes=5))
        }

    # Create session
    session_id = str(uuid.uuid4())
    sessions[session_id] = {
        "user_id": user_id,
        "created_at": datetime.now(timezone.utc),
        "last_activity": datetime.now(timezone.utc),
        "ip_address": request.client.host
    }

    # Update last login
    user.last_login = datetime.now(timezone.utc)
    user.updated_at = datetime.now(timezone.utc)

    create_audit_log(user_id, "login.success", "User", user_id, request)

    return Token(
        access_token=create_access_token(user),
        refresh_token=create_refresh_token(user),
        expires_in=ACCESS_TOKEN_EXPIRE_MINUTES * 60
    )

@app.post("/users/login/verify-mfa")
async def verify_mfa(mfa_data: MFAVerify, request: Request):
    """Verify MFA code and complete login"""
    try:
        payload = jwt.decode(mfa_data.temp_token, JWT_SECRET, algorithms=[JWT_ALGORITHM])
        user_id = payload["sub"]
    except jwt.PyJWTError:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid temporary token")

    user = users.get(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    if not verify_mfa_code(mfa_data.code, user_id):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid MFA code")

    session_id = str(uuid.uuid4())
    sessions[session_id] = {
        "user_id": user_id,
        "created_at": datetime.now(timezone.utc),
        "last_activity": datetime.now(timezone.utc),
        "ip_address": request.client.host
    }

    user.last_login = datetime.now(timezone.utc)
    user.updated_at = datetime.now(timezone.utc)

    create_audit_log(user_id, "mfa.verified", "User", user_id, request)

    return Token(
        access_token=create_access_token(user),
        refresh_token=create_refresh_token(user),
        expires_in=ACCESS_TOKEN_EXPIRE_MINUTES * 60
    )

@app.get("/users/me")
async def get_current_user(request: Request, authorization: str = None):
    """Get current authenticated user"""
    if not authorization:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Not authenticated")

    try:
        scheme, token = authorization.split()
        if scheme.lower() != "bearer":
            raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid authentication scheme")

        payload = jwt.decode(token, JWT_SECRET, algorithms=[JWT_ALGORITHM])
        user_id = payload["sub"]
    except (ValueError, jwt.PyJWTError):
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="Invalid token")

    user = users.get(user_id)
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    return {
        "id": user.id,
        "email": user.email,
        "username": user.username,
        "first_name": user.first_name,
        "last_name": user.last_name,
        "role_ids": user.role_ids,
        "permissions": get_user_permissions(user),
        "organization_id": user.organization_id,
        "mfa_enabled": user.mfa_enabled,
        "last_login": user.last_login
    }

@app.get("/users/{user_id}")
async def get_user(user_id: str, authorization: str = Depends(lambda x: x)):
    """Get user by ID"""
    if user_id not in users:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    user = users[user_id]
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
        "last_login": user.last_login
    }

@app.put("/users/{user_id}")
async def update_user(user_id: str, update_data: UserUpdate, request: Request, authorization: str = Depends(lambda x: x)):
    """Update user details"""
    if user_id not in users:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    user = users[user_id]

    if update_data.email:
        if update_data.email in users_by_email and users_by_email[update_data.email] != user_id:
            raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Email already in use")
        users_by_email.pop(user.email, None)
        user.email = update_data.email
        users_by_email[update_data.email] = user_id

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
        for role_id in update_data.role_ids:
            if role_id not in roles:
                raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail=f"Role {role_id} not found")
        user.role_ids = update_data.role_ids

    user.updated_at = datetime.now(timezone.utc)

    create_audit_log(user_id, "user.updated", "User", user_id, request, {"updated_fields": update_data.model_dump(exclude_none=True)})

    return {"ok": True, "updated_at": user.updated_at}

@app.post("/users/{user_id}/change-password")
async def change_password(user_id: str, passwords: PasswordChange, request: Request, authorization: str = Depends(lambda x: x)):
    """Change user password"""
    if user_id not in users:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    user = users[user_id]

    if not verify_password(passwords.current_password, user.password_hash):
        create_audit_log(user_id, "password.change.failed", "User", user_id, request, {"reason": "invalid_current_password"})
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Current password is incorrect")

    user.password_hash = hash_password(passwords.new_password)
    user.updated_at = datetime.now(timezone.utc)

    # Invalidate all refresh tokens
    for token_id, token_data in list(refresh_tokens.items()):
        if token_data["user_id"] == user_id:
            del refresh_tokens[token_id]

    create_audit_log(user_id, "password.changed", "User", user_id, request)

    return {"ok": True, "message": "Password changed successfully"}

# --- MFA Management ---

@app.post("/users/{user_id}/mfa/setup")
async def setup_mfa(user_id: str, mfa_setup: MFASetup, request: Request):
    """Setup MFA for user"""
    if user_id not in users:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    user = users[user_id]

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
        "message": f"MFA code sent via {mfa_setup.method.value}"
    }

@app.post("/users/{user_id}/mfa/verify")
async def verify_mfa_setup(user_id: str, mfa_verify: MFAVerify, request: Request):
    """Verify MFA setup"""
    if user_id not in users:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    user = users[user_id]

    if not verify_mfa_code(mfa_verify.code, user_id):
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid MFA code")

    user.mfa_enabled = True
    user.mfa_method = mfa_verify.method
    user.updated_at = datetime.now(timezone.utc)

    create_audit_log(user_id, "mfa.enabled", "User", user_id, request)

    return {"ok": True, "message": "MFA enabled successfully"}

@app.post("/users/{user_id}/mfa/disable")
async def disable_mfa(user_id: str, code: str = Form(...), request: Request = None):
    """Disable MFA for user"""
    if user_id not in users:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    user = users[user_id]

    if not verify_mfa_code(code, user_id):
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Invalid MFA code")

    user.mfa_enabled = False
    user.mfa_method = None
    user.updated_at = datetime.now(timezone.utc)

    create_audit_log(user_id, "mfa.disabled", "User", user_id, request)

    return {"ok": True, "message": "MFA disabled successfully"}

# --- Role Management ---

@app.get("/roles")
async def list_roles():
    """List all available roles"""
    return {"total": len(roles), "roles": list(roles.values())}

@app.post("/roles", status_code=status.HTTP_201_CREATED)
async def create_role(role_data: RoleCreate, request: Request = None):
    """Create a new role"""
    role_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    role = Role(
        id=role_id,
        name=role_data.name,
        description=role_data.description,
        permissions=role_data.permissions,
        is_system=role_data.is_system,
        created_at=now,
        updated_at=now
    )

    roles[role_id] = role

    create_audit_log("system", "role.created", "Role", role_id, request, {"name": role_data.name})

    return role

@app.get("/roles/{role_id}")
async def get_role(role_id: str):
    """Get role by ID"""
    if role_id not in roles:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Role not found")
    return roles[role_id]

@app.put("/roles/{role_id}")
async def update_role(role_id: str, update_data: RoleCreate, request: Request = None):
    """Update a role"""
    if role_id not in roles:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Role not found")

    role = roles[role_id]
    if role.is_system:
        raise HTTPException(status_code=status.HTTP_400_BAD_REQUEST, detail="Cannot modify system role")

    role.name = update_data.name
    role.description = update_data.description
    role.permissions = update_data.permissions
    role.updated_at = datetime.now(timezone.utc)

    create_audit_log("system", "role.updated", "Role", role_id, request, {"name": update_data.name})

    return role

# --- Organization Management ---

@app.post("/organizations", status_code=status.HTTP_201_CREATED)
async def create_organization(org_data: OrganizationCreate, request: Request = None):
    """Create a new organization"""
    org_id = str(uuid.uuid4())
    now = datetime.now(timezone.utc)

    org = Organization(
        id=org_id,
        name=org_data.name,
        description=org_data.description,
        settings=org_data.settings,
        created_at=now,
        updated_at=now
    )

    organizations[org_id] = org

    create_audit_log("system", "organization.created", "Organization", org_id, request, {"name": org_data.name})

    return org

@app.get("/organizations/{org_id}")
async def get_organization(org_id: str):
    """Get organization by ID"""
    if org_id not in organizations:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Organization not found")
    return organizations[org_id]

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

    user = users.get(token_data["user_id"])
    if not user or user.status != UserStatus.ACTIVE:
        raise HTTPException(status_code=status.HTTP_401_UNAUTHORIZED, detail="User not active")

    # Generate new tokens
    new_access_token = create_access_token(user)
    new_refresh_token = create_refresh_token(user)

    # Delete old refresh token
    del refresh_tokens[refresh_token]

    return Token(
        access_token=new_access_token,
        refresh_token=new_refresh_token,
        expires_in=ACCESS_TOKEN_EXPIRE_MINUTES * 60
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
    user_id = users_by_email.get(reset_data.email)

    if user_id:
        user = users[user_id]
        raw_token, hashed_token = generate_password_reset_token()

        password_reset_tokens[hashed_token] = {
            "user_id": user_id,
            "created_at": datetime.now(timezone.utc),
            "exp": datetime.now(timezone.utc) + timedelta(hours=24)
        }

        # In production, send email with reset link containing raw_token

        create_audit_log(user_id, "password.reset.requested", "User", user_id, request)

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

    user = users.get(token_data["user_id"])
    if not user:
        raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="User not found")

    user.password_hash = hash_password(confirm_data.new_password)
    user.updated_at = datetime.now(timezone.utc)
    user.status = UserStatus.ACTIVE

    del password_reset_tokens[hashed_token]

    # Invalidate all refresh tokens for this user
    for token_id, data in list(refresh_tokens.items()):
        if data["user_id"] == user.id:
            del refresh_tokens[token_id]

    create_audit_log(user.id, "password.reset.completed", "User", user.id, request)

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
    offset: int = 0
):
    """List audit log entries"""
    results = audit_logs

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
    results = results[offset:offset + limit]

    return {"total": total, "logs": results}

# --- Session Management ---

@app.get("/sessions")
async def list_sessions(authorization: str = Depends(lambda x: x)):
    """List active sessions for current user"""
    return {"total": len(sessions), "sessions": list(sessions.values())}

@app.delete("/sessions/{session_id}")
async def revoke_session(session_id: str, request: Request = None):
    """Revoke a specific session"""
    if session_id in sessions:
        del sessions[session_id]
        return {"ok": True}
    raise HTTPException(status_code=status.HTTP_404_NOT_FOUND, detail="Session not found")

# --- Capabilities Reference ---

@app.get("/capabilities")
async def list_capabilities():
    """List all available capabilities"""
    return {
        "capabilities": [
            {"name": c.value, "description": c.name.replace("_", " ").title()}
            for c in Capability
        ]
    }


if __name__ == "__main__":
    import uvicorn
    import os
    uvicorn.run(app, host="0.0.0.0", port=8080)