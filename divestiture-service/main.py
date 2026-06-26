"""
Divestiture Service
Port: 8291
Business divestiture analysis
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Divestiture Service", version="1.0.0")

class DivestitureCandidate(BaseModel):
    entity_id: str
    entity_name: str
    revenue: float
    ebitda: float
    book_value: float
    estimated_value: float

class DivestitureRequest(BaseModel):
    company_id: str
    candidates: List[DivestitureCandidate]
    strategic_fit_score: Dict[str, float]

class DivestitureResponse(BaseModel):
    company_id: str
    divestiture_analysis: List[Dict[str, Any]]
    recommendations: List[str]
    total_proceeds_estimate: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "divestiture", "version": "1.0.0"}

@app.post("/analyze", response_model=DivestitureResponse)
async def analyze_divestiture(request: DivestitureRequest):
    logger.info("Analyzing divestiture options", company=request.company_id)

    divestiture_analysis = []
    total_proceeds = 0
    
    for c in request.candidates:
        ebitda_multiple = c.estimated_value / c.ebitda if c.ebitda else 0
        fit_score = request.strategic_fit_score.get(c.entity_id, 0.5)
        
        divestiture_analysis.append({
            "entity_id": c.entity_id,
            "entity_name": c.entity_name,
            "revenue": round(c.revenue, 2),
            "ebitda": round(c.ebitda, 2),
            "book_value": round(c.book_value, 2),
            "estimated_value": round(c.estimated_value, 2),
            "ebitda_multiple": round(ebitda_multiple, 2),
            "strategic_fit": round(fit_score * 100, 2),
            "recommendation": "Divest" if fit_score < 0.4 else "Hold" if fit_score < 0.7 else "Keep"
        })
        
        if fit_score < 0.5:
            total_proceeds += c.estimated_value
    
    recommendations = []
    low_fit = [d for d in divestiture_analysis if d["strategic_fit"] < 40]
    if low_fit:
        recommendations.append(f"{len(low_fit)} entities with low strategic fit - consider divestiture")
    
    return DivestitureResponse(
        company_id=request.company_id,
        divestiture_analysis=divestiture_analysis,
        recommendations=recommendations,
        total_proceeds_estimate=round(total_proceeds, 2)
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8291)
