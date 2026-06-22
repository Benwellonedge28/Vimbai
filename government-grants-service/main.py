"""
Government Grants Service
Port: 8220
Grant recognition and compliance under IAS 20
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Government Grants Service", version="1.0.0")

class GrantItem(BaseModel):
    grant_id: str
    grant_name: str
    grant_type: str
    total_grant_amount: float
    amount_received: float
    amount Recognized: float
    deferred_income: float
    conditions_met: bool
    compliance_status: str

class GovernmentGrantsRequest(BaseModel):
    company_id: str
    period: str
    grants: List[Dict[str, Any]]
    related_assets: List[Dict[str, Any]]

class GovernmentGrantsResponse(BaseModel):
    company_id: str
    period: str
    grant_items: List[GrantItem]
    total_grant_revenue: float
    deferred_income_balance: float
    unmet_conditions: List[str]
    compliance_issues: List[str]
    recommendations: List[str]

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
    return {"status": "healthy", "service": "government-grants", "version": "1.0.0"}

@app.post("/analyze", response_model=GovernmentGrantsResponse)
async def analyze_government_grants(request: GovernmentGrantsRequest):
    logger.info("Analyzing government grants", company=request.company_id, period=request.period)

    grant_items = []
    total_revenue = 0.0
    deferred = 0.0
    unmet_conditions = []
    compliance_issues = []

    for grant in request.grants:
        conditions_met = grant.get("conditions_met", True)
        deferred_income = grant.get("deferred_income", 0)

        if not conditions_met:
            unmet_conditions.append(f"Grant {grant.get('id')}: {grant.get('name')}")

        if not grant.get("compliance_verified", True):
            compliance_issues.append(f"Compliance issue with grant {grant.get('id')}")

        recognized = grant.get("amount_received", 0) - deferred_income
        total_revenue += recognized
        deferred += deferred_income

        grant_items.append(GrantItem(
            grant_id=grant.get("id", ""),
            grant_name=grant.get("name", ""),
            grant_type=grant.get("type", ""),
            total_grant_amount=grant.get("total_amount", 0),
            amount_received=grant.get("amount_received", 0),
            amount_Recognized=recognized,
            deferred_income=deferred_income,
            conditions_met=conditions_met,
            compliance_status="compliant" if conditions_met and grant.get("compliance_verified", True) else "non_compliant"
        ))

    return GovernmentGrantsResponse(
        company_id=request.company_id,
        period=request.period,
        grant_items=grant_items,
        total_grant_revenue=round(total_revenue, 2),
        deferred_income_balance=round(deferred, 2),
        unmet_conditions=unmet_conditions if unmet_conditions else ["All grant conditions met"],
        compliance_issues=compliance_issues if compliance_issues else ["No compliance issues identified"],
        recommendations=[
            "Ensure conditions for all grants are properly assessed",
            "Recognize grant income in line with IAS 20 requirements",
            "Maintain documentation for audit purposes"
        ]
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8220)
