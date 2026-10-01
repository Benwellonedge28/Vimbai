"""Vimbai Subscription Plans Service - plan catalog and company subscriptions. Port: 8370

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "subscription_plans_service" not in _sys.modules or not hasattr(
    _sys.modules.get("subscription_plans_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("subscription_plans_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["subscription_plans_service"] = _pkg
    _sys.modules["subscription_plans_service"].__path__ = [_HERE]

import os
from datetime import datetime, timezone

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession
from subscription_plans_service import crud, models
from subscription_plans_service.dependencies import book_id_var, get_db_session, get_user_id
from subscription_plans_service.exceptions import SubscriptionPlansError

SERVICE_NAME = "subscription-plans-service"
PORT = int(os.getenv("PORT", "8370"))
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
app = FastAPI(title="Vimbai Subscription Plans Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.exception_handler(SubscriptionPlansError)
async def _subscription_plans_error(request: Request, exc: SubscriptionPlansError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}


@app.post("/plans", response_model=models.Plan)
async def create_plan(
    plan: models.Plan,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    item = await crud.create_plan(db_session, user_id, plan)
    logger.info("plan_created", tier=item.tier, name=item.name)
    return item


@app.get("/plans", response_model=models.List[models.Plan])
async def list_plans(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_plans(db_session, user_id)


@app.post("/subscribe", response_model=models.Subscription)
async def subscribe(
    company_id: str,
    plan_id: str,
    cycle: models.BillingCycle = models.BillingCycle.MONTHLY,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    plan = await crud.get_plan(db_session, user_id, plan_id)
    if not plan:
        raise HTTPException(status_code=404, detail="Plan not found")
    sub = await crud.create_subscription(db_session, user_id, company_id, plan, cycle)
    logger.info("subscription_created", company_id=company_id, plan=plan.name, cycle=cycle)
    return sub


@app.get("/subscriptions/{company_id}", response_model=models.List[models.Subscription])
async def list_subscriptions(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.list_subscriptions(db_session, user_id, company_id)


@app.post("/upgrade", response_model=models.UpgradeResult)
async def upgrade_plan(req: models.UpgradeRequest):
    """Proration math is pure computation and does not touch stored data."""
    tier_prices = {
        models.PlanTier.FREE: 0,
        models.PlanTier.BASIC: 49,
        models.PlanTier.PROFESSIONAL: 199,
        models.PlanTier.ENTERPRISE: 999,
    }
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

    return models.UpgradeResult(
        company_id=req.company_id,
        current_plan=req.current_plan.value,
        target_plan=req.target_plan.value,
        proration_amount=round(proration, 2),
        effective_date=datetime.now(timezone.utc).strftime("%Y-%m-%d"),
        new_billing_amount=round(target_price, 2),
        cycle="monthly",
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
