"""Group Tax Service - Port 8297"""
import httpx; import structlog; from typing import Any, Dict, List; from pydantic import BaseModel; from fastapi import FastAPI
logger = structlog.get_logger(); app = FastAPI(title="Group Tax Service", version="1.0.0")

class GroupTaxRequest(BaseModel):
    company_id: str; entities: List[Dict[str, Any]]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "group-tax"}

@app.post("/analyze", response_model=Dict[str, Any])
async def analyze_group_tax(request: GroupTaxRequest):
    logger.info("Analyzing group tax", company=request.company_id)
    total_tax = sum(e.get("tax", 0) for e in request.entities)
    return {"company_id": request.company_id, "total_entities": len(request.entities), "total_group_tax": total_tax}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=8297)
