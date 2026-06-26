"""
Capital Allocation Service
Port: 8289
Optimal capital distribution
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Capital Allocation Service", version="1.0.0")

class AllocationOpportunity(BaseModel):
    opportunity_id: str
    name: str
    required_capital: float
    expected_return: float
    priority: str

class CapitalAllocationRequest(BaseModel):
    company_id: str
    total_capital: float
    opportunities: List[AllocationOpportunity]
    constraints: Dict[str, float]

class CapitalAllocationResponse(BaseModel):
    company_id: str
    allocation_summary: Dict[str, Any]
    allocations: List[Dict[str, Any]]
    remaining_capital: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "capital-allocation", "version": "1.0.0"}

@app.post("/allocate", response_model=CapitalAllocationResponse)
async def allocate_capital(request: CapitalAllocationRequest):
    logger.info("Allocating capital", company=request.company_id)

    sorted_opps = sorted(request.opportunities, key=lambda x: (-x.priority.count("High"), -x.expected_return))
    
    allocations = []
    remaining = request.total_capital
    
    for opp in sorted_opps:
        if remaining >= opp.required_capital:
            allocations.append({
                "opportunity_id": opp.opportunity_id,
                "name": opp.name,
                "allocated": opp.required_capital,
                "expected_return": round(opp.expected_return * 100, 2),
                "priority": opp.priority
            })
            remaining -= opp.required_capital
    
    allocation_summary = {
        "total_capital": request.total_capital,
        "allocated": request.total_capital - remaining,
        "allocations_count": len(allocations)
    }
    
    return CapitalAllocationResponse(
        company_id=request.company_id,
        allocation_summary=allocation_summary,
        allocations=allocations,
        remaining_capital=round(remaining, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8289)
