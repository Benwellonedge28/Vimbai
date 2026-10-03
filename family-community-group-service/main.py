"""
Vimbai Family and Community Group Service
Manages family/community savings groups, contribution tracking, and payout
schedules. Caller-owned (X-User-Id) and Book-gated (X-Book-ID) stores
persist in Neo4j.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "family_community_group_service" not in _sys.modules or not hasattr(
    _sys.modules.get("family_community_group_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location(
        "family_community_group_service", _os.path.join(_HERE, "__init__.py")
    )
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["family_community_group_service"] = _pkg
    _sys.modules["family_community_group_service"].__path__ = [_HERE]

from typing import List, Optional

from family_community_group_service import crud
from family_community_group_service.dependencies import book_id_var, get_db_session, get_user_id
from family_community_group_service.models import (
    CommunityGroup,
    Contribution,
    GroupMember,
    PayoutSchedule,
)
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "family-community-group-service"
SERVICE_VERSION = "1.0.0"
PORT = int(_os.getenv("PORT", "8005"))

try:
    import structlog

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
except ImportError:  # pragma: no cover
    import logging

    logging.basicConfig(level=logging.INFO)
    logger = logging.getLogger(SERVICE_NAME)

app = FastAPI(title="Vimbai Family & Community Group Service", version=SERVICE_VERSION, docs_url="/docs")
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


@app.post("/groups", response_model=CommunityGroup)
async def create_group(
    name: str,
    description: str = "",
    contribution_frequency: str = "monthly",
    contribution_amount: float = 0.0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create a new family/community savings group."""
    valid_freqs = ["weekly", "biweekly", "monthly"]
    if contribution_frequency not in valid_freqs:
        raise HTTPException(status_code=400, detail=f"Invalid frequency. Must be one of {valid_freqs}")

    group = CommunityGroup(
        name=name,
        description=description,
        contribution_frequency=contribution_frequency,
        contribution_amount=contribution_amount,
    )
    saved = await crud.create_group(db_session, user_id, group)
    logger.info("Community group created", group_id=saved.id, name=name)
    return saved


@app.get("/groups", response_model=List[CommunityGroup])
async def list_groups(
    status: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all community groups (caller's own Book-visible groups)."""
    result = await crud.list_groups(db_session, user_id)
    if status:
        result = [g for g in result if g.status == status]
    return result


@app.get("/groups/{group_id}", response_model=CommunityGroup)
async def get_group(
    group_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get a specific community group."""
    group = await crud.get_group(db_session, user_id, group_id)
    if not group:
        raise HTTPException(status_code=404, detail="Group not found")
    return group


@app.post("/groups/{group_id}/members", response_model=GroupMember)
async def add_member(
    group_id: str,
    name: str,
    email: str = "",
    phone: str = "",
    contribution_amount: float = 0.0,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a member to a community group (caller's own groups only)."""
    group = await crud.get_group(db_session, user_id, group_id)
    if not group:
        raise HTTPException(status_code=404, detail="Group not found")

    member = GroupMember(
        name=name,
        email=email,
        phone=phone,
        contribution_amount=contribution_amount or group.contribution_amount,
    )
    saved = await crud.add_member(db_session, user_id, group_id, member)
    await _update_member_count(db_session, user_id, group_id)
    logger.info("Member added to group", group_id=group_id, member_id=saved.id)
    return saved


async def _update_member_count(db_session: AsyncSession, user_id: str, group_id: str) -> int:
    """Recompute member_count from the caller's member set and persist it on the group."""
    count = len(await crud.list_members(db_session, user_id, group_id))
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_GROUP]->(g:CommunityGroup)
    WHERE g.id = $group_id AND ($book_id IS NULL OR g.book_id = $book_id)
    SET g.member_count = toFloat($count)
    """
    from family_community_group_service.dependencies import book_id_var as _b

    await db_session.run(query, {"user_id": user_id, "group_id": group_id, "count": float(count), "book_id": _b.get()})
    return count


@app.get("/groups/{group_id}/members", response_model=List[GroupMember])
async def list_members(
    group_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List members of a community group (caller's own groups only)."""
    return await crud.list_members(db_session, user_id, group_id)


@app.post("/groups/{group_id}/contribute")
async def record_contribution(
    group_id: str,
    member_id: str,
    amount: float,
    notes: str = "",
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Record a member's contribution for the current cycle."""
    group = await crud.get_group(db_session, user_id, group_id)
    if not group:
        raise HTTPException(status_code=404, detail="Group not found")

    member = await crud.get_member(db_session, user_id, group_id, member_id)
    if not member:
        raise HTTPException(status_code=404, detail="Member not found in this group")

    contribution = Contribution(
        group_id=group_id,
        member_id=member_id,
        amount=amount,
        cycle_number=group.current_cycle,
        notes=notes,
    )
    contributions = await crud.list_contributions(db_session, user_id, group_id)
    new_pool = round(group.total_pool + amount, 10)
    saved = await crud.create_contribution(db_session, user_id, contribution, new_pool)
    logger.info("Contribution recorded", group_id=group_id, member_id=member_id, amount=amount)
    return saved


@app.get("/groups/{group_id}/contributions", response_model=List[Contribution])
async def list_contributions(
    group_id: str,
    cycle: Optional[int] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List contributions for a group, optionally filtered by cycle."""
    result = await crud.list_contributions(db_session, user_id, group_id)
    if cycle is not None:
        result = [c for c in result if c.cycle_number == cycle]
    return result


@app.post("/groups/{group_id}/advance-cycle")
async def advance_cycle(
    group_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Advance to the next contribution cycle."""
    group = await crud.get_group(db_session, user_id, group_id)
    if not group:
        raise HTTPException(status_code=404, detail="Group not found")

    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_GROUP]->(g:CommunityGroup)
    WHERE g.id = $group_id AND ($book_id IS NULL OR g.book_id = $book_id)
    SET g.current_cycle = toFloat($new_cycle)
    RETURN g
    """
    from family_community_group_service.dependencies import book_id_var as _b

    result = await db_session.run(
        query,
        {"user_id": user_id, "group_id": group_id, "new_cycle": float(group.current_cycle + 1), "book_id": _b.get()},
    )
    rec = await result.single()
    new_cycle = int(dict(rec["g"])["current_cycle"])
    logger.info("Cycle advanced", group_id=group_id, new_cycle=new_cycle)
    return {"group_id": group_id, "current_cycle": new_cycle}


@app.get("/groups/{group_id}/payouts", response_model=List[PayoutSchedule])
async def list_payouts(
    group_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List payout schedule for a group (caller's own groups only)."""
    return await crud.list_payouts(db_session, user_id, group_id)


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
