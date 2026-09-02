"""
Provisions and Contingencies Service
Port: 8205
Provision recognition and contingency disclosure
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Provisions and Contingencies Service", version="1.0.0")


class ProvisionItem(BaseModel):
    provision_id: str
    category: str
    description: str
    carrying_amount: float
    best_estimate: float
    possible_outcome: float
    recognition_status: str


class ProvisionsRequest(BaseModel):
    company_id: str
    audit_id: str
    provisions: List[Dict[str, Any]]
    contingencies: List[Dict[str, Any]]
    legal_advice_available: bool
    tax_uncertainties: List[Dict[str, Any]]


class ProvisionsResponse(BaseModel):
    company_id: str
    audit_id: str
    provisions_recognized: int
    total_provision_amount: float
    contingent_liabilities: int
    contingent_assets: int
    unrecognised_provisions: List[Dict[str, Any]]
    provision_items: List[ProvisionItem]
    tax_provisions: Dict[str, float]
    disclosure_completeness: str
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
    return {"status": "healthy", "service": "provisions-contingencies", "version": "1.0.0"}


@app.post("/assess", response_model=ProvisionsResponse)
async def assess_provisions_contingencies(request: ProvisionsRequest):
    logger.info("Assessing provisions and contingencies", company=request.company_id)

    provision_items = []
    total_amount = 0.0
    recognized_count = 0

    for prov in request.provisions:
        best = prov.get("best_estimate", 0.0)
        possible = prov.get("possible_outcome", 0.0)

        status = "recognized" if best > 0 and prov.get("probable", True) else "not_recognized"
        if status == "recognized":
            recognized_count += 1
            total_amount += best

        provision_items.append(
            ProvisionItem(
                provision_id=prov.get("id", ""),
                category=prov.get("category", ""),
                description=prov.get("description", ""),
                carrying_amount=prov.get("carrying_amount", 0.0),
                best_estimate=best,
                possible_outcome=possible,
                recognition_status=status,
            )
        )

    unrecognised = [
        {"id": p.provision_id, "description": p.description, "possible_amount": p.possible_outcome}
        for p in provision_items
        if p.recognition_status == "not_recognized"
    ]

    tax_provisions = {
        "current_tax": sum(t.get("current", 0) for t in request.tax_uncertainties),
        "deferred_tax": sum(t.get("deferred", 0) for t in request.tax_uncertainties),
        "uncertain_positions": len([t for t in request.tax_uncertainties if t.get("uncertain", False)]),
    }

    return ProvisionsResponse(
        company_id=request.company_id,
        audit_id=request.audit_id,
        provisions_recognized=recognized_count,
        total_provision_amount=round(total_amount, 2),
        contingent_liabilities=len([c for c in request.contingencies if c.get("type") == "liability"]),
        contingent_assets=len([c for c in request.contingencies if c.get("type") == "asset"]),
        unrecognised_provisions=unrecognised if unrecognised else [{"message": "No unrecognised provisions"}],
        provision_items=provision_items,
        tax_provisions=tax_provisions,
        disclosure_completeness="Complete" if recognized_count > 0 else "Review required",
        recommendations=[
            "Ensure provisions meet recognition criteria under IAS 37",
            "Review contingent liabilities for possible recognition",
            "Document estimation techniques for provisions",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8205)
