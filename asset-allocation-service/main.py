"""
Asset Allocation Service
Port: 8235
Strategic asset allocation
"""

from typing import Any, Dict, List

import httpx
import structlog
from fastapi import FastAPI
from pydantic import BaseModel

logger = structlog.get_logger()
app = FastAPI(title="Asset Allocation Service", version="1.0.0")


class AllocationResult(BaseModel):
    asset_class: str
    current_allocation: float
    target_allocation: float
    deviation: float
    rebalance_amount: float


class AssetAllocationRequest(BaseModel):
    company_id: str
    investor_profile: str
    time_horizon: int
    risk_tolerance: str
    current_portfolio: Dict[str, float]
    total_portfolio_value: float


class AssetAllocationResponse(BaseModel):
    company_id: str
    investor_profile: str
    allocation_results: List[AllocationResult]
    total_deviation: float
    rebalancing_required: bool
    strategic_allocation: Dict[str, float]
    tactical_allocation: Dict[str, float]
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
    return {"status": "healthy", "service": "asset-allocation", "version": "1.0.0"}


@app.post("/allocate", response_model=AssetAllocationResponse)
async def allocate_assets(request: AssetAllocationRequest):
    logger.info("Allocating assets", company=request.company_id, profile=request.investor_profile)

    target_allocations = {
        "conservative": {"equities": 0.3, "bonds": 0.5, "cash": 0.15, "alternatives": 0.05},
        "moderate": {"equities": 0.5, "bonds": 0.35, "cash": 0.1, "alternatives": 0.05},
        "aggressive": {"equities": 0.7, "bonds": 0.2, "cash": 0.05, "alternatives": 0.05},
    }

    targets = target_allocations.get(request.investor_profile, target_allocations["moderate"])
    allocation_results = []
    total_deviation = 0.0

    for asset_class, target in targets.items():
        current = (
            request.current_portfolio.get(asset_class, 0) / request.total_portfolio_value
            if request.total_portfolio_value
            else 0
        )
        deviation = abs(current - target)
        total_deviation += deviation

        allocation_results.append(
            AllocationResult(
                asset_class=asset_class,
                current_allocation=round(current * 100, 2),
                target_allocation=round(target * 100, 2),
                deviation=round(deviation * 100, 2),
                rebalance_amount=round((target - current) * request.total_portfolio_value, 2),
            )
        )

    rebalance_required = total_deviation > 0.05

    return AssetAllocationResponse(
        company_id=request.company_id,
        investor_profile=request.investor_profile,
        allocation_results=allocation_results,
        total_deviation=round(total_deviation * 100, 2),
        rebalancing_required=rebalance_required,
        strategic_allocation={k: round(v * 100, 2) for k, v in targets.items()},
        tactical_allocation={
            k: round(v * 105 if request.investor_profile == "aggressive" else v * 0.95, 2) for k, v in targets.items()
        },
        recommendations=[
            "Rebalance if deviation exceeds 5%",
            "Review allocation quarterly",
            "Consider tax implications of rebalancing",
        ],
    )


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=8235)
