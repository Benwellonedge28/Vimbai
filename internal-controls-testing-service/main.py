"""
Internal Controls Testing Service
Port: 8284
Internal controls testing and evaluation
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI
from datetime import datetime

logger = structlog.get_logger()
app = FastAPI(title="Internal Controls Testing Service", version="1.0.0")

class ControlTest(BaseModel):
    control_id: str
    control_description: str
    control_type: str
    frequency: str
    last_tested: str
    result: str

class InternalControlsTestingRequest(BaseModel):
    company_id: str
    controls: List[ControlTest]

class InternalControlsTestingResponse(BaseModel):
    company_id: str
    testing_date: str
    controls_summary: Dict[str, Any]
    test_results: List[Dict[str, Any]]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "internal-controls-testing", "version": "1.0.0"}

@app.post("/test", response_model=InternalControlsTestingResponse)
async def test_internal_controls(request: InternalControlsTestingRequest):
    logger.info("Testing internal controls", company=request.company_id)

    passed = sum(1 for c in request.controls if c.result == "Pass")
    failed = sum(1 for c in request.controls if c.result == "Fail")
    
    test_results = [
        {"control_id": c.control_id, "description": c.control_description, "type": c.control_type, "result": c.result}
        for c in request.controls
    ]
    
    controls_summary = {
        "total_controls": len(request.controls),
        "passed": passed,
        "failed": failed,
        "pass_rate": round(passed / len(request.controls) * 100, 2) if request.controls else 0
    }
    
    recommendations = []
    if failed > 0:
        recommendations.append(f"{failed} controls failed testing - remediation required")
    
    return InternalControlsTestingResponse(
        company_id=request.company_id,
        testing_date=datetime.now().isoformat(),
        controls_summary=controls_summary,
        test_results=test_results,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8284)
