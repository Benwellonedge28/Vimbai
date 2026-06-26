"""
Transfer Pricing Service
Port: 8294
Intercompany pricing analysis
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Transfer Pricing Service", version="1.0.0")

class TransferPricingRequest(BaseModel):
    company_id: str
    intercompany_transactions: List[Dict[str, Any]]
    arm_length_benchmark: Dict[str, float]

class TransferPricingResponse(BaseModel):
    company_id: str
    tp_analysis: List[Dict[str, Any]]
    risk_assessment: Dict[str, Any]
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "transfer-pricing", "version": "1.0.0"}

@app.post("/analyze", response_model=TransferPricingResponse)
async def analyze_transfer_pricing(request: TransferPricingRequest):
    logger.info("Analyzing transfer pricing", company=request.company_id)

    tp_analysis = []
    for tx in request.intercompany_transactions:
        benchmark = request.arm_length_benchmark.get(tx.get("transaction_type", "default"), 0.5)
        deviation = abs(tx.get("margin", 0) - benchmark) / benchmark if benchmark else 0
        
        tp_analysis.append({
            "transaction_id": tx.get("id", "Unknown"),
            "type": tx.get("transaction_type", "Unknown"),
            "amount": tx.get("amount", 0),
            "margin": tx.get("margin", 0),
            "benchmark": benchmark,
            "deviation_pct": round(deviation * 100, 2),
            "compliant": deviation < 0.1
        })
    
    non_compliant = sum(1 for t in tp_analysis if not t["compliant"])
    risk_assessment = {
        "total_transactions": len(tp_analysis),
        "non_compliant": non_compliant,
        "risk_level": "High" if non_compliant > 5 else "Medium" if non_compliant > 2 else "Low"
    }
    
    recommendations = []
    if non_compliant > 0:
        recommendations.append(f"{non_compliant} transactions deviate from arm's length - review pricing")
    
    return TransferPricingResponse(company_id=request.company_id, tp_analysis=tp_analysis, risk_assessment=risk_assessment, recommendations=recommendations)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8294)
