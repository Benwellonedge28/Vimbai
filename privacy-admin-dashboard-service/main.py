from typing import Any, Dict, List

import structlog
from fastapi import Depends, FastAPI, Header, HTTPException
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Privacy-Preserving Admin Dashboard Service", version="1.0.0")


class OperationalMetrics(BaseModel):
    registered_users: int
    active_users_monthly: int
    new_signups_weekly: int
    subscription_conversions: int
    app_crashes_weekly: int
    storage_usage_tb: float
    avg_app_startup_time_sec: float
    feature_usage_counts: Dict[str, int]


# Mock data representing operational information, NOT user finances
MOCK_METRICS = OperationalMetrics(
    registered_users=145000,
    active_users_monthly=89000,
    new_signups_weekly=2100,
    subscription_conversions=450,
    app_crashes_weekly=12,
    storage_usage_tb=45.2,
    avg_app_startup_time_sec=1.5,
    feature_usage_counts={"budget_creation": 10000, "invoice_generation": 4500, "receipt_scan": 22000},
)


def verify_admin(authorization: str = Header(None)):
    if not authorization or authorization != "Bearer admin_secret_token":
        raise HTTPException(status_code=403, detail="Forbidden: Admin access required")
    return True


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": "privacy-admin-dashboard-service", "version": "1.0.0"}


@app.get("/metrics/operational", response_model=OperationalMetrics)
async def get_operational_metrics(is_admin: bool = Depends(verify_admin)):
    """
    Returns only operational information.
    Strictly forbids returning any personal financial data like "User X spent $Y" or "Company Z revenue".
    """
    logger.info("Admin accessed operational metrics")
    return MOCK_METRICS


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8003)
