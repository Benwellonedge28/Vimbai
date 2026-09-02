"""
Audit Evidence Service
Port: 8201
Evidence gathering and evaluation
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Audit Evidence Service", version="1.0.0")


class EvidenceItem(BaseModel):
    evidence_id: str
    source: str
    type: str
    description: str
    obtained_date: str
    sufficiency: bool
    appropriateness: bool


class EvidenceEvaluationRequest(BaseModel):
    audit_id: str
    assertion: str
    required_evidence_types: List[str]
    evidence_items: List[Dict[str, Any]]
    reliability_factors: Dict[str, str]


class EvidenceEvaluationResponse(BaseModel):
    audit_id: str
    assertion: str
    evidence_gathered: int
    evidence_types_used: List[str]
    sufficiency_assessment: str
    appropriateness_assessment: str
    reliability_score: float
    evidence_items: List[EvidenceItem]
    conclusion: str
    additional_evidence_required: List[str]


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
    return {"status": "healthy", "service": "audit-evidence", "version": "1.0.0"}


@app.post("/evaluate", response_model=EvidenceEvaluationResponse)
async def evaluate_evidence(request: EvidenceEvaluationRequest):
    logger.info("Evaluating audit evidence", audit=request.audit_id, assertion=request.assertion)

    evidence_items = []
    sufficiency_count = 0
    appropriateness_count = 0

    for item in request.evidence_items:
        sufficiency = item.get("sufficiency", False)
        appropriateness = item.get("appropriateness", False)

        if sufficiency:
            sufficiency_count += 1
        if appropriateness:
            appropriateness_count += 1

        evidence_items.append(
            EvidenceItem(
                evidence_id=item.get("id", ""),
                source=item.get("source", ""),
                type=item.get("type", ""),
                description=item.get("description", ""),
                obtained_date=item.get("date", ""),
                sufficiency=sufficiency,
                appropriateness=appropriateness,
            )
        )

    reliability_score = (
        (sufficiency_count + appropriateness_count) / (2 * len(request.evidence_items)) if request.evidence_items else 0
    )

    sufficiency_assessment = "sufficient" if sufficiency_count >= len(request.evidence_items) * 0.7 else "insufficient"
    appropriateness_assessment = (
        "appropriate" if appropriateness_count >= len(request.evidence_items) * 0.8 else "concerns_noted"
    )

    additional_required = []
    if sufficiency_assessment == "insufficient":
        additional_required.append("Extend sample sizes")
    if appropriateness_assessment == "concerns_noted":
        additional_required.append("Obtain corroborating evidence from independent sources")

    return EvidenceEvaluationResponse(
        audit_id=request.audit_id,
        assertion=request.assertion,
        evidence_gathered=len(request.evidence_items),
        evidence_types_used=list(set([e.type for e in evidence_items])),
        sufficiency_assessment=sufficiency_assessment,
        appropriateness_assessment=appropriateness_assessment,
        reliability_score=round(reliability_score, 2),
        evidence_items=evidence_items,
        conclusion=(
            "Evidence supports assertion"
            if sufficiency_assessment == "sufficient" and appropriateness_assessment == "appropriate"
            else "Evidence requires supplementation"
        ),
        additional_evidence_required=(
            additional_required if additional_required else ["No additional evidence required"]
        ),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8201)
