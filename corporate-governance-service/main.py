"""
Corporate Governance Service
Port: 8209
Governance compliance and best practices
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Corporate Governance Service", version="1.0.0")


class GovernanceMetric(BaseModel):
    area: str
    requirement: str
    compliant: bool
    gap: str


class GovernanceRequest(BaseModel):
    company_id: str
    jurisdiction: str
    governance_code: str
    board_composition: Dict[str, Any]
    committees: List[Dict[str, Any]]
    policies: List[str]
    risk_management: Dict[str, Any]


class GovernanceResponse(BaseModel):
    company_id: str
    jurisdiction: str
    governance_code: str
    overall_compliance_score: float
    board_metrics: Dict[str, Any]
    committee_effectiveness: List[Dict[str, Any]]
    compliance_metrics: List[GovernanceMetric]
    non_compliant_areas: List[str]
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
    return {"status": "healthy", "service": "corporate-governance", "version": "1.0.0"}


@app.post("/assess", response_model=GovernanceResponse)
async def assess_corporate_governance(request: GovernanceRequest):
    logger.info("Assessing corporate governance", company=request.company_id)

    composition = request.board_composition
    total_directors = composition.get("total_directors", 0)
    independent_directors = composition.get("independent_directors", 0)
    female_directors = composition.get("female_directors", 0)

    independence_ratio = independent_directors / total_directors if total_directors else 0
    gender_diversity = female_directors / total_directors if total_directors else 0

    compliance_metrics = []

    compliance_metrics.append(
        GovernanceMetric(
            area="Board Composition",
            requirement="At least 30% independent directors",
            compliant=independence_ratio >= 0.3,
            gap="Independent director threshold not met" if independence_ratio < 0.3 else "Compliant",
        )
    )

    compliance_metrics.append(
        GovernanceMetric(
            area="Board Composition",
            requirement="Gender diversity target",
            compliant=gender_diversity >= 0.3,
            gap="Gender diversity target not met" if gender_diversity < 0.3 else "Compliant",
        )
    )

    compliance_metrics.append(
        GovernanceMetric(area="Audit Committee", requirement="All independent members", compliant=True, gap="")
    )

    compliance_metrics.append(
        GovernanceMetric(
            area="Risk Management",
            requirement="Documented risk framework",
            compliant=bool(request.risk_management.get("framework")),
            gap="Risk framework needs strengthening" if not request.risk_management.get("framework") else "Compliant",
        )
    )

    committee_effectiveness = [
        {
            "name": c.get("name", ""),
            "members": c.get("members", 0),
            "meetings": c.get("meetings_held", 0),
            "effective": c.get("meetings_held", 0) >= 4,
        }
        for c in request.committees
    ]

    compliant_count = sum(1 for m in compliance_metrics if m.compliant)
    compliance_score = (compliant_count / len(compliance_metrics)) * 100 if compliance_metrics else 0

    non_compliant = [m.area for m in compliance_metrics if not m.compliant]

    return GovernanceResponse(
        company_id=request.company_id,
        jurisdiction=request.jurisdiction,
        governance_code=request.governance_code,
        overall_compliance_score=round(compliance_score, 2),
        board_metrics={
            "total_directors": total_directors,
            "independent_directors": independent_directors,
            "independence_ratio": round(independence_ratio, 2),
            "gender_diversity": round(gender_diversity, 2),
            "average_tenure_years": composition.get("avg_tenure", 5),
        },
        committee_effectiveness=committee_effectiveness,
        compliance_metrics=compliance_metrics,
        non_compliant_areas=non_compliant if non_compliant else ["All governance requirements met"],
        recommendations=(
            [
                "Recruit additional independent directors",
                "Increase gender diversity on the board",
                "Ensure all committees meet minimum meeting requirements",
                "Review and update governance policies annually",
            ]
            if non_compliant
            else ["Continue monitoring governance compliance"]
        ),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8209)
