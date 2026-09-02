"""Vimbai Sensitivity Analysis Service - What-if analysis on financial variables. Port: 8325"""

import os
import uuid
from collections import defaultdict
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "sensitivity-analysis-service"
PORT = int(os.getenv("PORT", "8325"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Sensitivity Analysis Service", version="2.0.0", docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name="sensitivity-analysis-service", instrument_app=app)
except ImportError:
    TRACER = None


class Variable(BaseModel):
    name: str
    base_value: float
    change_pct: float = 0  # percentage change to test


class SensitivityResult(BaseModel):
    variable_name: str
    base_value: float
    changed_value: float
    change_pct: float
    impact_on_target: float
    elasticity: float = 0  # % change in target / % change in variable


class AnalysisRequest(BaseModel):
    company_id: str
    target_metric: str  # e.g., "net_profit", "cash_flow", "revenue"
    base_target_value: float
    variables: List[Variable]
    change_steps: List[float] = [-10, -5, 0, 5, 10]  # percentage changes to test


class AnalysisResponse(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    target_metric: str
    base_target_value: float
    results: List[SensitivityResult]
    most_sensitive_variable: str = ""
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


_analyses: Dict[str, List[AnalysisResponse]] = defaultdict(list)


def estimate_impact(var: Variable, target: str, base_target: float) -> float:
    """Estimate impact of variable change on target metric using simplified linear model."""
    change_pct = var.change_pct / 100
    if target in ("net_profit", "profit"):
        if "revenue" in var.name.lower():
            return base_target * (1 + change_pct * 0.6)
        elif "cost" in var.name.lower() or "expense" in var.name.lower():
            return base_target * (1 - change_pct * 0.4)
        elif "interest" in var.name.lower():
            return base_target * (1 - change_pct * 0.1)
        return base_target * (1 + change_pct * 0.2)
    elif target in ("cash_flow", "cashflow"):
        if "revenue" in var.name.lower():
            return base_target * (1 + change_pct * 0.7)
        elif "cost" in var.name.lower():
            return base_target * (1 - change_pct * 0.5)
        return base_target * (1 + change_pct * 0.3)
    elif target == "revenue":
        if "price" in var.name.lower():
            return base_target * (1 + change_pct * 0.8)
        elif "volume" in var.name.lower() or "sales" in var.name.lower():
            return base_target * (1 + change_pct * 0.9)
        return base_target * (1 + change_pct * 0.3)
    return base_target * (1 + change_pct * 0.2)


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/analyze", response_model=AnalysisResponse)
async def run_analysis(req: AnalysisRequest):
    results = []
    for var in req.variables:
        for step in req.change_steps:
            var_copy = Variable(name=var.name, base_value=var.base_value, change_pct=step)
            changed_value = var.base_value * (1 + step / 100)
            impact = estimate_impact(var_copy, req.target_metric, req.base_target_value)
            elasticity = (
                (step / 100) / ((impact - req.base_target_value) / max(1, req.base_target_value))
                if impact != req.base_target_value
                else 0
            )
            results.append(
                SensitivityResult(
                    variable_name=var.name,
                    base_value=var.base_value,
                    changed_value=changed_value,
                    change_pct=step,
                    impact_on_target=impact,
                    elasticity=abs(elasticity),
                )
            )

    # Find most sensitive variable (highest avg elasticity)
    var_elasticity = defaultdict(list)
    for r in results:
        var_elasticity[r.variable_name].append(r.elasticity)
    most_sensitive = max(var_elasticity, key=lambda v: sum(var_elasticity[v]) / len(var_elasticity[v]), default="")

    resp = AnalysisResponse(
        company_id=req.company_id,
        target_metric=req.target_metric,
        base_target_value=req.base_target_value,
        results=results,
        most_sensitive_variable=most_sensitive,
    )
    _analyses[req.company_id].append(resp)
    return resp


@app.get("/analyses/{company_id}")
async def get_analyses(company_id: str):
    return {
        "company_id": company_id,
        "analyses": _analyses.get(company_id, []),
        "total": len(_analyses.get(company_id, [])),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
