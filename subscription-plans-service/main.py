"""
Vimbai Subscription Plans Service
Subscription tier management, billing cycles, and plan upgrade/downgrade logic.
Port: 8398
"""
import os, uuid
from datetime import datetime, timezone, timedelta
from typing import Dict, List, Optional
from enum import Enum
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "subscription-plans-service"
PORT = int(os.getenv("PORT", "8398"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Subscription Plans Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class BillingCycle(str, Enum):
    MONTHLY = "monthly"; QUARTERLY = "quarterly"; ANNUAL = "annual"

class PlanTier(str, Enum):
    FREE = "free"; BASIC = "basic"; PROFESSIONAL = "professional"; ENTERPRISE = "enterprise"

class Plan(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    tier: PlanTier; name: str; price_monthly: float
    features: List[str] = []; max_users: int = 5
    max_companies: int = 1; api_calls_per_month: int = 1000

class Subscription(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; plan_id: str; tier: PlanTier
    billing_cycle: BillingCycle = BillingCycle.MONTHLY
    start_date: str = Field(default_factory=lambda: datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    status: str = "active"  # active, cancelled, suspended, past_due
    current_period_end: str = ""

class UpgradeRequest(BaseModel):
    company_id: str; current_plan: PlanTier; target_plan: PlanTier
    current_period_end: str; prorate: bool = True

class UpgradeResult(BaseModel):
    company_id: str; current_plan: str; target_plan: str
    proration_amount: float; effective_date: str
    new_billing_amount: float; cycle: str

_plans: Dict[str, Plan] = {}
_subs: Dict[str, Subscription] = {}

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/plans", response_model=Plan)
async def create_plan(plan: Plan):
    _plans[plan.id] = plan
    return plan

@app.get("/plans", response_model=List[Plan])
async def list_plans():
    return list(_plans.values())

@app.post("/subscribe", response_model=Subscription)
async def subscribe(company_id: str, plan_id: str, cycle: BillingCycle = BillingCycle.MONTHLY):
    plan = _plans.get(plan_id)
    if not plan:
        from fastapi import HTTPException; raise HTTPException(status_code=404, detail="Plan not found")
    
    days = {"monthly": 30, "quarterly": 90, "annual": 365}.get(cycle.value, 30)
    end = (datetime.now(timezone.utc) + timedelta(days=days)).strftime("%Y-%m-%d")
    
    sub = Subscription(
        company_id=company_id, plan_id=plan_id, tier=plan.tier,
        billing_cycle=cycle, current_period_end=end
    )
    _subs[sub.id] = sub
    return sub

@app.post("/upgrade", response_model=UpgradeResult)
async def upgrade_plan(req: UpgradeRequest):
    tier_prices = {PlanTier.FREE: 0, PlanTier.BASIC: 49, PlanTier.PROFESSIONAL: 199, PlanTier.ENTERPRISE: 999}
    current_price = tier_prices.get(req.current_plan, 0)
    target_price = tier_prices.get(req.target_plan, 0)
    
    proration = 0
    if req.prorate:
        try:
            end = datetime.fromisoformat(req.current_period_end.replace("Z", "+00:00"))
            remaining_days = max((end - datetime.now(timezone.utc)).days, 0)
            daily_current = current_price / 30
            daily_target = target_price / 30
            proration = round((daily_target - daily_current) * remaining_days, 2)
        except Exception:
            proration = 0
    
    return UpgradeResult(
        company_id=req.company_id,
        current_plan=req.current_plan.value, target_plan=req.target_plan.value,
        proration_amount=round(proration, 2),
        effective_date=datetime.now(timezone.utc).strftime("%Y-%m-%d"),
        new_billing_amount=round(target_price, 2),
        cycle="monthly"
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
