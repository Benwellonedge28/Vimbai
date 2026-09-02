"""
Zero-Based Budget Service
Port: 8176
Zero-based budgeting - all expenses justified
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Zero-Based Budget Service", version="1.0.0")


class ExpenseItem(BaseModel):
    item_id: str
    description: str
    department: str
    requested_amount: float
    justification: str
    mandatory: bool


class ZeroBasedBudgetRequest(BaseModel):
    company_id: str
    budget_year: str
    total_available: float
    expense_items: List[ExpenseItem]


class PrioritizedItem(BaseModel):
    item_id: str
    description: str
    department: str
    requested_amount: float
    priority_score: float
    funded: bool
    funded_amount: float


class ZeroBasedBudgetResponse(BaseModel):
    company_id: str
    budget_year: str
    total_available: float
    total_requested: float
    funded_items: List[PrioritizedItem]
    unfunded_items: List[PrioritizedItem]
    funding_coverage: float


async def call_internal_service(service_url: str, endpoint: str, data: dict = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{service_url}{endpoint}"
            response = await client.post(url, json=data) if data else await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception as e:
        logger.warning(f"Failed to call {service_url}{endpoint}: {e}")
        return {}


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "zero-based-budget", "version": "1.0.0"}


@app.post("/prepare", response_model=ZeroBasedBudgetResponse)
async def prepare_zero_based_budget(request: ZeroBasedBudgetRequest):
    logger.info("Preparing zero-based budget", company=request.company_id)

    prioritized = []
    for item in request.expense_items:
        priority = 100 if item.mandatory else 50
        prioritized.append(
            PrioritizedItem(
                item_id=item.item_id,
                description=item.description,
                department=item.department,
                requested_amount=item.requested_amount,
                priority_score=priority,
                funded=False,
                funded_amount=0.0,
            )
        )

    prioritized.sort(key=lambda x: x.priority_score, reverse=True)

    remaining = request.total_available
    funded = []
    unfunded = []

    for item in prioritized:
        if item.requested_amount <= remaining:
            item.funded = True
            item.funded_amount = item.requested_amount
            remaining -= item.requested_amount
            funded.append(item)
        else:
            unfunded.append(item)

    return ZeroBasedBudgetResponse(
        company_id=request.company_id,
        budget_year=request.budget_year,
        total_available=request.total_available,
        total_requested=sum(i.requested_amount for i in request.expense_items),
        funded_items=funded,
        unfunded_items=unfunded,
        funding_coverage=len(funded) / len(prioritized) * 100 if prioritized else 0,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8176)
