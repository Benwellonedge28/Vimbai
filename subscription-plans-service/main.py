from typing import Dict, Any, List, Optional
from pydantic import BaseModel
from fastapi import FastAPI, HTTPException
import structlog

logger = structlog.get_logger()
app = FastAPI(title="Subscription Plans Service", version="1.0.0")

class SubscriptionPlan(BaseModel):
    plan_id: str
    name: str
    target_users: str
    features: List[str]
    price_monthly: float

PLANS = {
    "free": SubscriptionPlan(
        plan_id="free", name="Free", target_users="Individual users getting started",
        features=["Basic budgeting", "Expense tracking", "On-device encryption"], price_monthly=0.0
    ),
    "family": SubscriptionPlan(
        plan_id="family", name="Family", target_users="Families or small groups managing shared finances",
        features=["2-10 users", "Shared household budgets", "Shared savings goals", "Bill reminders", "Expense splitting", "Role-based permissions"], price_monthly=9.99
    ),
    "basic": SubscriptionPlan(
        plan_id="basic", name="Basic", target_users="Sole traders and small businesses",
        features=["Invoicing", "Basic reports", "Tax estimation"], price_monthly=19.99
    ),
    "professional": SubscriptionPlan(
        plan_id="professional", name="Professional", target_users="Growing businesses",
        features=["Multi-currency", "Advanced reports", "Inventory tracking"], price_monthly=39.99
    ),
    "business": SubscriptionPlan(
        plan_id="business", name="Business", target_users="Medium-sized companies",
        features=["Payroll integration", "API access", "Priority support"], price_monthly=99.99
    ),
    "enterprise": SubscriptionPlan(
        plan_id="enterprise", name="Enterprise", target_users="Large organizations",
        features=["Single Sign-On (SSO)", "Dedicated account manager", "Custom integrations"], price_monthly=299.99
    ),
    "government": SubscriptionPlan(
        plan_id="government", name="Government", target_users="Public sector organizations",
        features=["Compliance reporting", "Enhanced audit trails", "On-premise deployment options"], price_monthly=499.99
    )
}

@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": "subscription-plans-service", "version": "1.0.0"}

@app.get("/plans", response_model=List[SubscriptionPlan])
async def list_plans():
    logger.info("Fetching all subscription plans")
    return list(PLANS.values())

@app.get("/plans/{plan_id}", response_model=SubscriptionPlan)
async def get_plan(plan_id: str):
    plan = PLANS.get(plan_id.lower())
    if not plan:
        raise HTTPException(status_code=404, detail="Plan not found")
    return plan

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8001)
