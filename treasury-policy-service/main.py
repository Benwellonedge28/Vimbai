"""
Treasury Policy Service
Port: 8263
Treasury policy compliance and governance
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Treasury Policy Service", version="1.0.0")


class TreasuryPolicyRequest(BaseModel):
    company_id: str
    policy_limits: Dict[str, Any]
    current_positions: Dict[str, float]
    transactions: List[Dict[str, Any]]


class TreasuryPolicyResponse(BaseModel):
    company_id: str
    policy_summary: Dict[str, Any]
    compliance_status: Dict[str, Any]
    violations: List[str]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "treasury-policy", "version": "1.0.0"}


@app.post("/assess", response_model=TreasuryPolicyResponse)
async def assess_treasury_policy(request: TreasuryPolicyRequest):
    logger.info("Assessing treasury policy", company=request.company_id)

    violations = []
    compliance_items = []

    for key, limit in request.policy_limits.items():
        current = request.current_positions.get(key, 0)
        if isinstance(limit, dict):
            max_val = limit.get("max", float("inf"))
            min_val = limit.get("min", 0)
            if current > max_val:
                violations.append(f"{key} exceeds maximum: {current} > {max_val}")
            elif current < min_val:
                violations.append(f"{key} below minimum: {current} < {min_val}")
            else:
                compliance_items.append(
                    {"item": key, "status": "Compliant", "utilization": round(current / max_val * 100, 2)}
                )

    policy_summary = {
        "total_policies": len(request.policy_limits),
        "compliant_count": len(compliance_items),
        "violation_count": len(violations),
    }

    compliance_status = {
        "overall_compliance": "GREEN" if len(violations) == 0 else "AMBER" if len(violations) < 3 else "RED",
        "compliance_items": compliance_items,
    }

    recommendations = []
    if violations:
        recommendations.append("Policy violations require immediate attention")
    if len(violations) > 5:
        recommendations.append("Multiple violations - review policy adequacy")

    return TreasuryPolicyResponse(
        company_id=request.company_id,
        policy_summary=policy_summary,
        compliance_status=compliance_status,
        violations=violations,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8263)
