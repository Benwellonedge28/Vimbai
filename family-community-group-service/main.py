from typing import Any, Dict, List

import structlog
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Family & Community Group Service", version="1.0.0")


class GroupMember(BaseModel):
    user_id: str
    role: str  # e.g., 'parent', 'child', 'treasurer', 'auditor', 'member'


class Group(BaseModel):
    group_id: str
    name: str
    group_type: str  # 'family', 'club', 'church', 'sports_team', 'community'
    members: List[GroupMember]
    features_enabled: List[str]


# Mock database
GROUPS = {}


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": "family-community-group-service", "version": "1.0.0"}


@app.post("/groups", response_model=Group)
async def create_group(group: Group):
    logger.info("Creating new group", group_type=group.group_type)
    GROUPS[group.group_id] = group
    return group


@app.get("/groups/{group_id}", response_model=Group)
async def get_group(group_id: str):
    if group_id not in GROUPS:
        raise HTTPException(status_code=404, detail="Group not found")
    return GROUPS[group_id]


@app.post("/groups/{group_id}/contributions")
async def collect_contribution(group_id: str, amount: float, member_id: str):
    """Allows clubs/churches to collect member contributions."""
    if group_id not in GROUPS:
        raise HTTPException(status_code=404, detail="Group not found")
    logger.info("Contribution collected", group_id=group_id, amount=amount)
    return {"status": "success", "message": f"Collected {amount} from {member_id}"}


@app.post("/groups/{group_id}/expenses/split")
async def split_expense(group_id: str, total_amount: float, split_among: List[str]):
    """Expense splitting for families or groups."""
    if group_id not in GROUPS:
        raise HTTPException(status_code=404, detail="Group not found")

    split_amount = round(total_amount / len(split_among), 2)
    return {
        "status": "success",
        "total": total_amount,
        "split_amount_per_person": split_amount,
        "members_involved": split_among,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8005)
