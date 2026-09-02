"""
Director Emoluments Service
Port: 8208
Director compensation and benefits disclosure
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Director Emoluments Service", version="1.0.0")


class DirectorEmoluments(BaseModel):
    director_id: str
    director_name: str
    role: str
    base_salary: float
    bonuses: float
    share_options_value: float
    pension_contributions: float
    benefits: float
    total_emoluments: float


class EmolumentsRequest(BaseModel):
    company_id: str
    period: str
    directors: List[Dict[str, Any]]
    highest_paid_director: str
    employee_count: int
    total_staff_costs: float


class EmolumentsResponse(BaseModel):
    company_id: str
    period: str
    director_emoluments: List[DirectorEmoluments]
    total_emoluments: float
    highest_paid: float
    average_emoluments: float
    emoluments_to_staff_costs: float
    ceo_pay_ratio: float
    disclosure_compliant: bool
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
    return {"status": "healthy", "service": "director-emoluments", "version": "1.0.0"}


@app.post("/analyze", response_model=EmolumentsResponse)
async def analyze_director_emoluments(request: EmolumentsRequest):
    logger.info("Analyzing director emoluments", company=request.company_id, period=request.period)

    director_list = []
    total = 0.0
    highest = 0.0
    highest_id = ""

    for director in request.directors:
        base = director.get("base_salary", 0)
        bonus = director.get("bonuses", 0)
        options = director.get("share_options_value", 0)
        pension = director.get("pension_contributions", 0)
        benefits = director.get("benefits", 0)
        total_dir = base + bonus + options + pension + benefits

        total += total_dir

        if total_dir > highest:
            highest = total_dir
            highest_id = director.get("id", "")

        director_list.append(
            DirectorEmoluments(
                director_id=director.get("id", ""),
                director_name=director.get("name", ""),
                role=director.get("role", ""),
                base_salary=base,
                bonuses=bonus,
                share_options_value=options,
                pension_contributions=pension,
                benefits=benefits,
                total_emoluments=total_dir,
            )
        )

    avg_emoluments = total / len(request.directors) if request.directors else 0
    emoluments_ratio = total / request.total_staff_costs if request.total_staff_costs else 0
    ceo_ratio = highest / avg_emoluments if avg_emoluments else 0

    return EmolumentsResponse(
        company_id=request.company_id,
        period=request.period,
        director_emoluments=director_list,
        total_emoluments=round(total, 2),
        highest_paid=round(highest, 2),
        average_emoluments=round(avg_emoluments, 2),
        emoluments_to_staff_costs=round(emoluments_ratio, 4),
        ceo_pay_ratio=round(ceo_ratio, 2),
        disclosure_compliant=len(director_list) == len(request.directors),
        recommendations=[
            "Ensure all director emoluments are fully disclosed",
            "Include equity-settled share-based payments at fair value",
            "Disclose performance criteria for bonus payments",
            "Consider CEO pay ratio disclosure per regulatory requirements",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8208)
