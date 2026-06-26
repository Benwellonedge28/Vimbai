"""
Performance Benchmarking Service
Port: 8280
Industry benchmarking analysis
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="Performance Benchmarking Service", version="1.0.0")

class BenchmarkMetric(BaseModel):
    metric_name: str
    company_value: float
    industry_average: float
    industry_best: float

class PerformanceBenchmarkingRequest(BaseModel):
    company_id: str
    industry: str
    metrics: List[BenchmarkMetric]

class PerformanceBenchmarkingResponse(BaseModel):
    company_id: str
    industry: str
    benchmark_results: List[Dict[str, Any]]
    competitive_position: str
    recommendations: List[str]

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "performance-benchmarking", "version": "1.0.0"}

@app.post("/benchmark", response_model=PerformanceBenchmarkingResponse)
async def benchmark_performance(request: PerformanceBenchmarkingRequest):
    logger.info("Benchmarking performance", company=request.company_id, industry=request.industry)

    benchmark_results = []
    above_avg = 0
    
    for m in request.metrics:
        vs_avg = ((m.company_value - m.industry_average) / m.industry_average * 100) if m.industry_average else 0
        vs_best = ((m.company_value - m.industry_best) / m.industry_best * 100) if m.industry_best else 0
        
        if m.company_value > m.industry_average:
            above_avg += 1
        
        benchmark_results.append({
            "metric": m.metric_name,
            "company": round(m.company_value, 2),
            "industry_avg": round(m.industry_average, 2),
            "industry_best": round(m.industry_best, 2),
            "vs_average_pct": round(vs_avg, 2),
            "vs_best_pct": round(vs_best, 2),
            "position": "Above Average" if vs_avg > 0 else "Below Average"
        })
    
    position_pct = above_avg / len(request.metrics) * 100 if request.metrics else 0
    competitive_position = "Leader" if position_pct > 75 else "Above Average" if position_pct > 50 else "Below Average" if position_pct > 25 else "Laggard"
    
    recommendations = []
    if position_pct < 50:
        recommendations.append("Performance below industry average - prioritize improvement initiatives")
    
    return PerformanceBenchmarkingResponse(
        company_id=request.company_id,
        industry=request.industry,
        benchmark_results=benchmark_results,
        competitive_position=competitive_position,
        recommendations=recommendations
    )

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8280)
