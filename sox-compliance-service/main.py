"""
SOX Compliance Service
Port: 8286
Sarbanes-Oxley compliance management
"""

from datetime import datetime
from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="SOX Compliance Service", version="1.0.0")


class SOXControl(BaseModel):
    control_id: str
    section: str
    description: str
    testing_status: str
    deficiency: str


class SOXComplianceRequest(BaseModel):
    company_id: str
    controls: List[SOXControl]
    fiscal_year: str


class SOXComplianceResponse(BaseModel):
    company_id: str
    fiscal_year: str
    compliance_summary: Dict[str, Any]
    deficiency_summary: Dict[str, Any]
    recommendations: List[str]


@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "sox-compliance", "version": "1.0.0"}


@app.post("/assess", response_model=SOXComplianceResponse)
async def assess_sox_compliance(request: SOXComplianceRequest):
    logger.info("Assessing SOX compliance", company=request.company_id)

    tested = sum(1 for c in request.controls if c.testing_status == "Tested")
    deficiency_major = sum(1 for c in request.controls if c.deficiency == "Major")
    deficiency_material = sum(1 for c in request.controls if c.deficiency == "Material")

    compliance_summary = {
        "total_controls": len(request.controls),
        "tested": tested,
        "untested": len(request.controls) - tested,
        "deficiency_major": deficiency_major,
        "deficiency_material": deficiency_material,
        "testing_completion": round(tested / len(request.controls) * 100, 2) if request.controls else 0,
    }

    deficiency_summary = {
        "total_deficiencies": deficiency_major + deficiency_material,
        "material_weakness": deficiency_material,
        "significant_deficiency": deficiency_major,
    }

    recommendations = []
    if deficiency_material > 0:
        recommendations.append("Material weaknesses identified - immediate remediation required")
    if deficiency_major > 0:
        recommendations.append("Significant deficiencies require management attention")
    if compliance_summary["testing_completion"] < 100:
        recommendations.append("Complete remaining control testing before year-end")

    return SOXComplianceResponse(
        company_id=request.company_id,
        fiscal_year=request.fiscal_year,
        compliance_summary=compliance_summary,
        deficiency_summary=deficiency_summary,
        recommendations=recommendations,
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8286)
