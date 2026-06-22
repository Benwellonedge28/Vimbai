"""
Operational Risk Service
Port: 8166
Operational risk measurement, loss data analysis, key risk indicators
"""
import httpx
import structlog
from typing import Any, Dict, List, Optional
from pydantic import BaseModel, Field
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Operational Risk Service", version="1.0.0")

class LossEvent(BaseModel):
    event_id: str
    event_type: str
    loss_amount: float
    event_date: str
    business_line: str

class KRI(BaseModel):
    indicator_id: str
    indicator_name: str
    current_value: float
    threshold_green: float
    threshold_amber: float
    threshold_red: float

class OperationalRiskRequest(BaseModel):
    company_id: str
    reporting_date: str
    loss_events: List[LossEvent]
    kris: List[KRI]
    business_line_revenues: Dict[str, float]

class OperationalRiskResponse(BaseModel):
    company_id: str
    total_losses: float
    average_loss: float
    max_loss: float
    loss_count: int
    var_99_9: float
    regulatory_capital: float
    kri_status: Dict[str, str]
    risk_rating: str

async def call_internal_service(service_url: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
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
    return {"status": "healthy", "service": "operational-risk", "version": "1.0.0"}

@app.post("/assess", response_model=OperationalRiskResponse)
async def assess_operational_risk(request: OperationalRiskRequest):
    logger.info("Assessing operational risk", company=request.company_id)

    total_losses = sum(e.loss_amount for e in request.loss_events)
    avg_loss = total_losses / len(request.loss_events) if request.loss_events else 0
    max_loss = max((e.loss_amount for e in request.loss_events), default=0)

    import math
    var_99_9 = max_loss * 2.5

    beta_factor = 12.5
    regulatory_capital = var_99_9 * beta_factor

    kri_status = {}
    for kri in request.kris:
        if kri.current_value <= kri.threshold_green:
            kri_status[kri.indicator_name] = "GREEN"
        elif kri.current_value <= kri.threshold_amber:
            kri_status[kri.indicator_name] = "AMBER"
        else:
            kri_status[kri.indicator_name] = "RED"

    risk_rating = "LOW" if sum(1 for s in kri_status.values() if s == "RED") == 0 else "MEDIUM" if sum(1 for s in kri_status.values() if s == "RED") <= 2 else "HIGH"

    return OperationalRiskResponse(
        company_id=request.company_id,
        total_losses=total_losses,
        average_loss=avg_loss,
        max_loss=max_loss,
        loss_count=len(request.loss_events),
        var_99_9=var_99_9,
        regulatory_capital=regulatory_capital,
        kri_status=kri_status,
        risk_rating=risk_rating
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8166)
