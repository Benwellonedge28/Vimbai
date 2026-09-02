"""
Internal Control Evaluation Service
Port: 8200
Control testing and deficiency assessment
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Internal Control Evaluation Service", version="1.0.0")


class ControlTestResult(BaseModel):
    control_id: str
    control_description: str
    control_type: str
    testing_result: str
    deficiency: str
    severity: str


class ControlEvaluationRequest(BaseModel):
    audit_id: str
    company_id: str
    control_framework: str
    controls_to_test: List[Dict[str, Any]]
    testing_approach: str


class ControlEvaluationResponse(BaseModel):
    audit_id: str
    controls_tested: int
    effective_controls: int
    deficiencies_found: int
    material_weaknesses: int
    significant_deficiencies: int
    control_test_results: List[ControlTestResult]
    overall_assessment: str
    remediation_recommendations: List[str]


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
    return {"status": "healthy", "service": "internal-control-evaluation", "version": "1.0.0"}


@app.post("/evaluate", response_model=ControlEvaluationResponse)
async def evaluate_internal_controls(request: ControlEvaluationRequest):
    logger.info("Evaluating internal controls", audit=request.audit_id, company=request.company_id)

    test_results = []
    material_weaknesses = 0
    significant_deficiencies = 0

    for control in request.controls_to_test:
        deficiency = control.get("deficiency", "none")
        severity = "none"

        if deficiency == "material":
            severity = "material_weakness"
            material_weaknesses += 1
        elif deficiency == "significant":
            severity = "significant_deficiency"
            significant_deficiencies += 1

        test_results.append(
            ControlTestResult(
                control_id=control.get("id", ""),
                control_description=control.get("description", ""),
                control_type=control.get("type", "preventive"),
                testing_result="ineffective" if deficiency != "none" else "effective",
                deficiency=deficiency,
                severity=severity,
            )
        )

    effective_count = len([r for r in test_results if r.testing_result == "effective"])

    return ControlEvaluationResponse(
        audit_id=request.audit_id,
        controls_tested=len(request.controls_to_test),
        effective_controls=effective_count,
        deficiencies_found=len(request.controls_to_test) - effective_count,
        material_weaknesses=material_weaknesses,
        significant_deficiencies=significant_deficiencies,
        control_test_results=test_results,
        overall_assessment=(
            "effective"
            if material_weaknesses == 0 and significant_deficiencies == 0
            else "ineffective_with_deficiencies"
        ),
        remediation_recommendations=(
            [
                "Implement segregation of duties for payment processing",
                "Strengthen access controls to financial systems",
                "Enhance management review procedures",
            ]
            if material_weaknesses > 0
            else ["Continue monitoring control effectiveness"]
        ),
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8200)
