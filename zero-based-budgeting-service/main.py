"""Vimbai Zero-Based Budgeting Service - Build budgets from zero with justification. Port: 8326"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "zero-based-budgeting-service"
PORT = int(os.getenv("PORT", "8326"))
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
app = FastAPI(title="Vimbai Zero-Based Budgeting Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="zero-based-budgeting-service", instrument_app=app)
except ImportError:
    TRACER = None


class ZBBStatus(str, Enum):
    DRAFT = "draft"
    SUBMITTED = "submitted"
    REVIEWED = "reviewed"
    APPROVED = "approved"
    REJECTED = "rejected"


class BudgetItem(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    department: str
    cost_center: str = ""
    category: str
    description: str
    amount: float
    justification: str
    priority: int = 3  # 1 (highest) to 5 (lowest)
    alternative_options: str = ""
    impact_if_cut: str = ""
    status: str = "pending"


class ZBBPackage(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    period: str  # e.g., "2026-Q1"
    name: str
    department: str
    items: List[BudgetItem] = []
    total_amount: float = 0
    status: ZBBStatus = ZBBStatus.DRAFT
    reviewer: str = ""
    review_notes: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


_packages: Dict[str, List[ZBBPackage]] = defaultdict(list)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/packages", response_model=ZBBPackage)
async def create_package(pkg: ZBBPackage):
    pkg.total_amount = sum(item.amount for item in pkg.items)
    _packages[pkg.company_id].append(pkg)
    logger.info("zbb_package_created", company_id=pkg.company_id, department=pkg.department, total=pkg.total_amount)
    return pkg


@app.get("/packages/{company_id}")
async def get_packages(company_id: str, department: Optional[str] = None, status_filter: Optional[str] = None):
    pkgs = _packages.get(company_id, [])
    if department:
        pkgs = [p for p in pkgs if p.department == department]
    if status_filter:
        pkgs = [p for p in pkgs if p.status.value == status_filter]
    return {"company_id": company_id, "packages": pkgs, "total": len(pkgs)}


@app.post("/packages/{package_id}/items")
async def add_item(package_id: str, item: BudgetItem):
    for pkgs in _packages.values():
        for p in pkgs:
            if p.id == package_id:
                p.items.append(item)
                p.total_amount = sum(i.amount for i in p.items)
                return {"package_id": package_id, "item_id": item.id, "total_amount": p.total_amount}
    raise HTTPException(status_code=404, detail="Package not found")


@app.put("/packages/{package_id}/status")
async def update_status(package_id: str, status: ZBBStatus, reviewer: str = "", notes: str = ""):
    for pkgs in _packages.values():
        for p in pkgs:
            if p.id == package_id:
                p.status = status
                if reviewer:
                    p.reviewer = reviewer
                if notes:
                    p.review_notes = notes
                return {"id": package_id, "status": status.value}
    raise HTTPException(status_code=404, detail="Package not found")


@app.put("/items/{item_id}/priority")
async def set_item_priority(item_id: str, priority: int, status: str = ""):
    if priority < 1 or priority > 5:
        raise HTTPException(status_code=400, detail="Priority must be 1-5")
    for pkgs in _packages.values():
        for p in pkgs:
            for item in p.items:
                if item.id == item_id:
                    item.priority = priority
                    if status:
                        item.status = status
                    return {"item_id": item_id, "priority": priority, "status": item.status}
    raise HTTPException(status_code=404, detail="Item not found")


@app.get("/summary/{company_id}")
async def zbb_summary(company_id: str):
    pkgs = _packages.get(company_id, [])
    if not pkgs:
        return {"company_id": company_id, "total_packages": 0, "total_budget": 0, "by_department": {}, "by_status": {}}
    total = sum(p.total_amount for p in pkgs)
    by_dept = defaultdict(float)
    by_status = defaultdict(int)
    for p in pkgs:
        by_dept[p.department] += p.total_amount
        by_status[p.status.value] += 1
    return {
        "company_id": company_id,
        "total_packages": len(pkgs),
        "total_budget": total,
        "by_department": dict(by_dept),
        "by_status": dict(by_status),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
