"""
Audit Sampling Service
Port: 8196
Statistical and non-statistical sampling methods
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from pydantic import BaseModel
from fastapi import FastAPI
import random

logger = structlog.get_logger()
app = FastAPI(title="Audit Sampling Service", version="1.0.0")

class SamplingParameters(BaseModel):
    population_size: int
    expected_error_rate: float
    tolerable_error_rate: float
    confidence_level: float
    sampling_method: str

class SampleItem(BaseModel):
    item_id: str
    transaction_date: str
    amount: float
    description: str
    risk_score: int

class AuditSamplingRequest(BaseModel):
    audit_id: str
    account_id: str
    population: List[Dict[str, Any]]
    parameters: SamplingParameters
    stratification: bool
    random_seed: Optional[int] = 42

class AuditSamplingResponse(BaseModel):
    audit_id: str
    account_id: str
    sample_size: int
    sample: List[SampleItem]
    stratification_levels: Dict[str, int]
    sampling_interval: float
    confidence_level: float
    margin_of_error: float
    statistical_conclusion: str

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
    return {"status": "healthy", "service": "audit-sampling", "version": "1.0.0"}

@app.post("/sample", response_model=AuditSamplingResponse)
async def create_audit_sample(request: AuditSamplingRequest):
    logger.info("Creating audit sample", audit=request.audit_id, account=request.account_id)

    pop_size = len(request.population)
    conf_factor = 1.96 if request.parameters.confidence_level >= 95 else 1.645

    if request.parameters.sampling_method == "attribute":
        n = (conf_factor ** 2 * 0.5 * 0.5) / (request.parameters.tolerable_error_rate ** 2)
        n = min(n, pop_size)
    else:
        n = pop_size * (request.parameters.expected_error_rate * (1 - request.parameters.expected_error_rate)) / (conf_factor ** 2 * request.parameters.tolerable_error_rate ** 2)
        n = min(max(n, 30), pop_size)

    if request.stratification:
        stratified = {"high": [], "medium": [], "low": []}
        for item in request.population:
            risk = item.get("risk_score", 50)
            if risk >= 70:
                stratified["high"].append(item)
            elif risk >= 40:
                stratified["medium"].append(item)
            else:
                stratified["low"].append(item)

        sample_size_per_stratum = {
            "high": max(int(n * 0.5), len(stratified["high"])),
            "medium": max(int(n * 0.35), len(stratified["medium"])),
            "low": max(int(n * 0.15), len(stratified["low"]))
        }

        sample_items = []
        for stratum, size in sample_size_per_stratum.items():
            items = stratified[stratum][:size]
            for item in items:
                sample_items.append(SampleItem(
                    item_id=item.get("item_id", ""),
                    transaction_date=item.get("date", ""),
                    amount=item.get("amount", 0.0),
                    description=item.get("description", ""),
                    risk_score=item.get("risk_score", 50)
                ))
    else:
        random.seed(request.random_seed)
        sampled_indices = random.sample(range(pop_size), min(int(n), pop_size))
        sample_items = [
            SampleItem(
                item_id=request.population[i].get("item_id", ""),
                transaction_date=request.population[i].get("date", ""),
                amount=request.population[i].get("amount", 0.0),
                description=request.population[i].get("description", ""),
                risk_score=request.population[i].get("risk_score", 50)
            )
            for i in sampled_indices
        ]

    interval = pop_size / len(sample_items) if sample_items else 1
    margin = request.parameters.tolerable_error_rate * 0.5

    return AuditSamplingResponse(
        audit_id=request.audit_id,
        account_id=request.account_id,
        sample_size=len(sample_items),
        sample=sample_items,
        stratification_levels={"high": len([s for s in sample_items if s.risk_score >= 70]),
                              "medium": len([s for s in sample_items if 40 <= s.risk_score < 70]),
                              "low": len([s for s in sample_items if s.risk_score < 40])},
        sampling_interval=round(interval, 2),
        confidence_level=request.parameters.confidence_level,
        margin_of_error=round(margin, 4),
        statistical_conclusion=f"Sample of {len(sample_items)} items provides {int(request.parameters.confidence_level * 100)}% confidence"
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8196)
