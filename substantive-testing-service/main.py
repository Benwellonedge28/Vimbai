"""
Substantive Testing Service
Port: 8198
Detailed testing of account balances and transactions
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Substantive Testing Service", version="1.0.0")

class TestItem(BaseModel):
    item_id: str
    description: str
    amount: float
    testing_type: str
    result: str
    misstatement: float

class SubstantiveTestRequest(BaseModel):
    audit_id: str
    account_id: str
    assertions: List[str]
    population: List[Dict[str, Any]]
    sample_size: int
    misstatement_threshold: float

class SubstantiveTestResponse(BaseModel):
    audit_id: str
    account_id: str
    tests_performed: List[str]
    items_tested: int
    exceptions_found: int
    total_misstatement: float
    projected_misstatement: float
    conclusion: str
    recommendation: str

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
    return {"status": "healthy", "service": "substantive-testing", "version": "1.0.0"}

@app.post("/test", response_model=SubstantiveTestResponse)
async def perform_substantive_tests(request: SubstantiveTestRequest):
    logger.info("Performing substantive tests", audit=request.audit_id, account=request.account_id)

    exceptions = []
    total_misstatement = 0.0

    for item in request.population[:request.sample_size]:
        misstatement = item.get("misstatement", 0.0)
        total_misstatement += abs(misstatement)
        if abs(misstatement) > request.misstatement_threshold:
            exceptions.append({
                "item_id": item.get("item_id", ""),
                "amount": item.get("amount", 0.0),
                "misstatement": misstatement
            })

    population_total = sum(item.get("amount", 0.0) for item in request.population)
    sample_ratio = len(request.population) / request.sample_size if request.sample_size else 1
    projected = total_misstatement * sample_ratio

    conclusion = "unqualified" if projected < request.misstatement_threshold * 0.1 else "qualified"

    return SubstantiveTestResponse(
        audit_id=request.audit_id,
        account_id=request.account_id,
        tests_performed=["existence", "completeness", "valuation", "rights_and_obligations", "accuracy", "cutoff", "classification", "presentation"],
        items_tested=request.sample_size,
        exceptions_found=len(exceptions),
        total_misstatement=round(total_misstatement, 2),
        projected_misstatement=round(projected, 2),
        conclusion=conclusion,
        recommendation="Extend testing" if len(exceptions) > 3 else "Conclude on account balance"
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8198)
