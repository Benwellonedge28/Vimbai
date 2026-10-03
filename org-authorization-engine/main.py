"""
Vimbai Organization Authorization Engine
Manages role-based access control (RBAC) for multi-tenant organizations.

Roles and role assignments persist in Neo4j, stamped with the caller
(X-User-Id) and the Book context (X-Book-ID, verified upstream by the
API gateway). Role lookups and permission checks resolve only against
the caller's own Book-visible records. The dead `permissions` list
(defined in the original mock but read by no endpoint) was dropped.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys
import uuid
from datetime import datetime, timezone
from typing import List, Optional

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "org_authorization_engine" not in _sys.modules or not hasattr(
    _sys.modules.get("org_authorization_engine"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("org_authorization_engine", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["org_authorization_engine"] = _pkg
    _sys.modules["org_authorization_engine"].__path__ = [_HERE]

import structlog
from fastapi import Depends, FastAPI, HTTPException, Query, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from org_authorization_engine import crud
from org_authorization_engine.database import Neo4jConnector
from org_authorization_engine.dependencies import book_id_var, get_db_session, get_user_id
from org_authorization_engine.models import CheckRequest, Role, UserAssignment
from pydantic import BaseModel, Field

SERVICE_NAME = "org-authorization-engine"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8008"))

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/roles", response_model=Role)
async def create_role(
    name: str,
    description: str = "",
    permissions: List[str] = Query(default=[]),
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new role."""
    role = Role(name=name, description=description, permissions=permissions)
    created = await crud.create_role(db_session, user_id, role)
    logger.info("Role created", role_id=created.id, name=name)
    return created


@app.get("/roles", response_model=List[Role])
async def list_roles(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all roles."""
    return await crud.list_roles(db_session, user_id)


@app.post("/assign", response_model=UserAssignment)
async def assign_role(
    user_id_param: str = Query(..., alias="user_id"),
    org_id: str = Query(...),
    role_id: str = Query(...),
    assigned_by: str = Query(default=""),
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Assign a role to a user within an organization."""
    role = await crud.get_role(db_session, caller_id, role_id)
    if not role:
        raise HTTPException(status_code=404, detail="Role not found")

    existing = [
        a
        for a in await crud.list_assignments(db_session, caller_id)
        if a.user_id == user_id_param and a.org_id == org_id and a.role_id == role_id
    ]
    if existing:
        raise HTTPException(status_code=409, detail="Role already assigned to this user in this org")

    assignment = UserAssignment(
        user_id=user_id_param,
        org_id=org_id,
        role_id=role_id,
        assigned_by=assigned_by,
    )
    created = await crud.create_assignment(db_session, caller_id, assignment)
    logger.info("Role assigned", user_id=user_id_param, org_id=org_id, role_id=role_id)
    return created


@app.delete("/assign/{assignment_id}")
async def revoke_role(
    assignment_id: str,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Revoke a role assignment."""
    assignment = await crud.get_assignment(db_session, caller_id, assignment_id)
    if not assignment:
        raise HTTPException(status_code=404, detail="Assignment not found")
    await crud.delete_assignment(db_session, caller_id, assignment_id)
    return {"revoked": True, "assignment_id": assignment_id}


@app.get("/user/{user_id}/roles", response_model=List[UserAssignment])
async def get_user_roles(
    user_id: str,
    org_id: Optional[str] = None,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get role assignments for a user, optionally filtered by org."""
    result = [a for a in await crud.list_assignments(db_session, caller_id) if a.user_id == user_id]
    if org_id:
        result = [a for a in result if a.org_id == org_id]
    return result


@app.post("/check")
async def check_permission(
    request: CheckRequest,
    caller_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Check if a user has a specific permission in an org."""
    roles = {r.id: r for r in await crud.list_roles(db_session, caller_id)}
    user_assignments = [
        a
        for a in await crud.list_assignments(db_session, caller_id)
        if a.user_id == request.user_id and a.org_id == request.org_id
    ]
    if not user_assignments:
        return {"allowed": False, "reason": "No role assignments found"}

    for assignment in user_assignments:
        role = roles.get(assignment.role_id)
        if role and request.permission in role.permissions:
            return {"allowed": True, "role": role.name, "permission": request.permission}

    return {"allowed": False, "reason": "Permission not granted by any assigned role"}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
