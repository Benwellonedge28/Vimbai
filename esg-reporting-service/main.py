"""
ESG Reporting Service
Port: 8230
Environmental, Social, and Governance reporting
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="ESG Reporting Service", version="1.0.0")


class ESGMetric(BaseModel):
    metric_name: str
    category: str
    value: float
    unit: str
    target: float
    status: str


class ESGReportingRequest(BaseModel):
    company_id: str
    reporting_period: str
    framework: str
    environmental_metrics: List[Dict[str, Any]]
    social_metrics: List[Dict[str, Any]]
    governance_metrics: List[Dict[str, Any]]


class ESGReportingResponse(BaseModel):
    company_id: str
    reporting_period: str
    framework: str
    environmental_metrics: List[ESGMetric]
    social_metrics: List[ESGMetric]
    governance_metrics: List[ESGMetric]
    overall_esg_score: float
    rating_agency_comparison: Dict[str, str]
    gaps_identified: List[str]
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
    return {"status": "healthy", "service": "esg-reporting", "version": "1.0.0"}


@app.post("/report", response_model=ESGReportingResponse)
async def prepare_esg_report(request: ESGReportingRequest):
    logger.info("Preparing ESG report", company=request.company_id, period=request.reporting_period)

    env_metrics = []
    soc_metrics = []
    gov_metrics = []
    gaps = []

    for m in request.environmental_metrics:
        status = "on_track" if m.get("value", 0) <= m.get("target", float("inf")) else "below_target"
        if status == "below_target":
            gaps.append(f"Environmental: {m.get('name')} target not met")
        env_metrics.append(
            ESGMetric(
                metric_name=m.get("name", ""),
                category="Environmental",
                value=m.get("value", 0),
                unit=m.get("unit", ""),
                target=m.get("target", 0),
                status=status,
            )
        )

    for m in request.social_metrics:
        status = "on_track" if m.get("value", 0) >= m.get("target", 0) else "below_target"
        if status == "below_target":
            gaps.append(f"Social: {m.get('name')} target not met")
        soc_metrics.append(
            ESGMetric(
                metric_name=m.get("name", ""),
                category="Social",
                value=m.get("value", 0),
                unit=m.get("unit", ""),
                target=m.get("target", 0),
                status=status,
            )
        )

    for m in request.governance_metrics:
        status = "compliant" if m.get("value", 0) >= m.get("target", 0) else "non_compliant"
        if status == "non_compliant":
            gaps.append(f"Governance: {m.get('name')} below threshold")
        gov_metrics.append(
            ESGMetric(
                metric_name=m.get("name", ""),
                category="Governance",
                value=m.get("value", 0),
                unit=m.get("unit", ""),
                target=m.get("target", 0),
                status=status,
            )
        )

    total_metrics = len(env_metrics) + len(soc_metrics) + len(gov_metrics)
    on_track = len([m for m in env_metrics + soc_metrics + gov_metrics if m.status in ["on_track", "compliant"]])
    esg_score = (on_track / total_metrics) * 100 if total_metrics else 0

    return ESGReportingResponse(
        company_id=request.company_id,
        reporting_period=request.reporting_period,
        framework=request.framework,
        environmental_metrics=env_metrics,
        social_metrics=soc_metrics,
        governance_metrics=gov_metrics,
        overall_esg_score=round(esg_score, 2),
        rating_agency_comparison={"msci_rating": "A", "cdp_score": "B+", "sustainalytics_risk": "Low"},
        gaps_identified=gaps if gaps else ["All ESG targets on track"],
        recommendations=(
            [
                "Address identified gaps in next reporting period",
                "Set ambitious but achievable targets",
                "Improve data collection processes",
            ]
            if gaps
            else ["Continue stakeholder engagement", "Enhance ESG disclosures"]
        ),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8230)
