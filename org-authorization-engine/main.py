"""
Vimbai Organization Authorization Engine
Manages role-based access control (RBAC) for multi-tenant organizations.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, Query, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "org-authorization-engine"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8008"))

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

app = FastAPI(title="Vimbai Organization Authorization Engine", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class Role(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    permissions: List[str] = []
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class UserAssignment(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    user_id: str
    org_id: str
    role_id: str
    assigned_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    assigned_by: str = ""


class Permission(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    resource: str
    action: str  # read, write, delete, admin


class CheckRequest(BaseModel):
    user_id: str
    org_id: str
    permission: str
    resource: str = ""


roles: List[Role] = []
assignments: List[UserAssignment] = []
permissions: List[Permission] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/roles", response_model=Role)
async def create_role(name: str, description: str = "", permissions: List[str] = Query(default=[])):
    """Create a new role."""
    role = Role(name=name, description=description, permissions=permissions)
    roles.append(role)
    logger.info("Role created", role_id=role.id, name=name)
    return role


@app.get("/roles", response_model=List[Role])
async def list_roles():
    """List all roles."""
    return roles


@app.post("/assign", response_model=UserAssignment)
async def assign_role(user_id: str, org_id: str, role_id: str, assigned_by: str = ""):
    """Assign a role to a user within an organization."""
    role = next((r for r in roles if r.id == role_id), None)
    if not role:
        raise HTTPException(status_code=404, detail="Role not found")

    existing = next(
        (a for a in assignments if a.user_id == user_id and a.org_id == org_id and a.role_id == role_id),
        None,
    )
    if existing:
        raise HTTPException(status_code=409, detail="Role already assigned to this user in this org")

    assignment = UserAssignment(
        user_id=user_id,
        org_id=org_id,
        role_id=role_id,
        assigned_by=assigned_by,
    )
    assignments.append(assignment)
    logger.info("Role assigned", user_id=user_id, org_id=org_id, role_id=role_id)
    return assignment


@app.delete("/assign/{assignment_id}")
async def revoke_role(assignment_id: str):
    """Revoke a role assignment."""
    global assignments
    assignment = next((a for a in assignments if a.id == assignment_id), None)
    if not assignment:
        raise HTTPException(status_code=404, detail="Assignment not found")

    assignments = [a for a in assignments if a.id != assignment_id]
    return {"revoked": True, "assignment_id": assignment_id}


@app.get("/user/{user_id}/roles", response_model=List[UserAssignment])
async def get_user_roles(user_id: str, org_id: Optional[str] = None):
    """Get role assignments for a user, optionally filtered by org."""
    result = [a for a in assignments if a.user_id == user_id]
    if org_id:
        result = [a for a in result if a.org_id == org_id]
    return result


@app.post("/check")
async def check_permission(request: CheckRequest):
    """Check if a user has a specific permission in an org."""
    user_assignments = [a for a in assignments if a.user_id == request.user_id and a.org_id == request.org_id]
    if not user_assignments:
        return {"allowed": False, "reason": "No role assignments found"}

    for assignment in user_assignments:
        role = next((r for r in roles if r.id == assignment.role_id), None)
        if role and request.permission in role.permissions:
            return {"allowed": True, "role": role.name, "permission": request.permission}

    return {"allowed": False, "reason": "Permission not granted by any assigned role"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
