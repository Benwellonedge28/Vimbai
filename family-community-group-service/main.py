"""
Vimbai Family and Community Group Service
Manages family/community savings groups, contribution tracking, and payout schedules.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "family-community-group-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8005"))

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

app = FastAPI(title="Vimbai Family & Community Group Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class GroupMember(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    email: str = ""
    phone: str = ""
    contribution_amount: float = 0.0
    joined_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    active: bool = True


class Contribution(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    group_id: str
    member_id: str
    amount: float
    contribution_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    cycle_number: int
    notes: str = ""


class CommunityGroup(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    contribution_frequency: str = "monthly"  # weekly, biweekly, monthly
    contribution_amount: float
    member_count: int = 0
    current_cycle: int = 1
    total_pool: float = 0.0
    status: str = "active"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class PayoutSchedule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    group_id: str
    member_id: str
    cycle_number: int
    payout_amount: float
    status: str = "scheduled"  # scheduled, paid, missed
    scheduled_date: datetime
    paid_at: Optional[datetime] = None


groups: List[CommunityGroup] = []
members: Dict[str, List[GroupMember]] = {}
contributions: List[Contribution] = []
payouts: List[PayoutSchedule] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/groups", response_model=CommunityGroup)
async def create_group(
    name: str, description: str = "", contribution_frequency: str = "monthly", contribution_amount: float = 0.0
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
    groups.append(group)
    members[group.id] = []
    logger.info("Community group created", group_id=group.id, name=name)
    return group


@app.get("/groups", response_model=List[CommunityGroup])
async def list_groups(status: Optional[str] = None):
    """List all community groups."""
    if status:
        return [g for g in groups if g.status == status]
    return groups


@app.get("/groups/{group_id}", response_model=CommunityGroup)
async def get_group(group_id: str):
    """Get a specific community group."""
    group = next((g for g in groups if g.id == group_id), None)
    if not group:
        raise HTTPException(status_code=404, detail="Group not found")
    return group


@app.post("/groups/{group_id}/members", response_model=GroupMember)
async def add_member(group_id: str, name: str, email: str = "", phone: str = "", contribution_amount: float = 0.0):
    """Add a member to a community group."""
    group = next((g for g in groups if g.id == group_id), None)
    if not group:
        raise HTTPException(status_code=404, detail="Group not found")

    member = GroupMember(
        name=name,
        email=email,
        phone=phone,
        contribution_amount=contribution_amount or group.contribution_amount,
    )
    members[group_id].append(member)
    group.member_count = len(members[group_id])
    logger.info("Member added to group", group_id=group_id, member_id=member.id)
    return member


@app.get("/groups/{group_id}/members", response_model=List[GroupMember])
async def list_members(group_id: str):
    """List members of a community group."""
    return members.get(group_id, [])


@app.post("/groups/{group_id}/contribute", response_model=Contribution)
async def record_contribution(group_id: str, member_id: str, amount: float, notes: str = ""):
    """Record a member's contribution for the current cycle."""
    group = next((g for g in groups if g.id == group_id), None)
    if not group:
        raise HTTPException(status_code=404, detail="Group not found")

    group_members = members.get(group_id, [])
    member = next((m for m in group_members if m.id == member_id), None)
    if not member:
        raise HTTPException(status_code=404, detail="Member not found in this group")

    contribution = Contribution(
        group_id=group_id,
        member_id=member_id,
        amount=amount,
        cycle_number=group.current_cycle,
        notes=notes,
    )
    contributions.append(contribution)
    group.total_pool += amount
    logger.info("Contribution recorded", group_id=group_id, member_id=member_id, amount=amount)
    return contribution


@app.get("/groups/{group_id}/contributions", response_model=List[Contribution])
async def list_contributions(group_id: str, cycle: Optional[int] = None):
    """List contributions for a group, optionally filtered by cycle."""
    result = [c for c in contributions if c.group_id == group_id]
    if cycle is not None:
        result = [c for c in result if c.cycle_number == cycle]
    return result


@app.post("/groups/{group_id}/advance-cycle")
async def advance_cycle(group_id: str):
    """Advance to the next contribution cycle and generate payout schedule."""
    group = next((g for g in groups if g.id == group_id), None)
    if not group:
        raise HTTPException(status_code=404, detail="Group not found")

    group.current_cycle += 1
    logger.info("Cycle advanced", group_id=group_id, new_cycle=group.current_cycle)
    return {"group_id": group_id, "current_cycle": group.current_cycle}


@app.get("/groups/{group_id}/payouts", response_model=List[PayoutSchedule])
async def list_payouts(group_id: str):
    """List payout schedule for a group."""
    return [p for p in payouts if p.group_id == group_id]


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
