"""
Vimbai Lifecycle Costing Service
Calculates total cost of ownership across asset lifecycle phases.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "lifecycle-costing-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8432"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Lifecycle Costing Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class LifecyclePhase(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    phase: str  # acquisition, operation, maintenance, disposal
    description: str = ""
    duration_years: int = 0
    annual_cost: float = 0.0
    one_time_cost: float = 0.0


class LifecycleCostModel(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    asset_name: str
    description: str = ""
    phases: List[LifecyclePhase] = []
    total_lifecycle_cost: float = 0.0
    annual_equivalent_cost: float = 0.0
    discount_rate: float = 0.1
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


cost_models: List[LifecycleCostModel] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/models", response_model=LifecycleCostModel)
async def create_model(
    asset_name: str,
    description: str = "",
    discount_rate: float = 0.1,
    phases: List[Dict[str, Any]] = [],
):
    """Create a lifecycle cost model with phases."""
    phase_list = [LifecyclePhase(**p) for p in phases]

    total = 0.0
    for p in phase_list:
        total += p.one_time_cost + (p.annual_cost * p.duration_years)

    total_duration = sum(p.duration_years for p in phase_list)
    annual_equiv = (total / total_duration) if total_duration > 0 else 0.0

    model = LifecycleCostModel(
        asset_name=asset_name,
        description=description,
        phases=phase_list,
        total_lifecycle_cost=total,
        annual_equivalent_cost=round(annual_equiv, 2),
        discount_rate=discount_rate,
    )
    cost_models.append(model)
    logger.info("Lifecycle cost model created", model_id=model.id, asset=asset_name, total=total)
    return model


@app.get("/models", response_model=List[LifecycleCostModel])
async def list_models():
    """List all lifecycle cost models."""
    return cost_models


@app.get("/models/{model_id}", response_model=LifecycleCostModel)
async def get_model(model_id: str):
    """Get a specific cost model."""
    model = next((m for m in cost_models if m.id == model_id), None)
    if not model:
        raise HTTPException(status_code=404, detail="Cost model not found")
    return model


@app.post("/compare")
async def compare_models(model_ids: List[str]):
    """Compare multiple lifecycle cost models."""
    models_to_compare = [m for m in cost_models if m.id in model_ids]
    if not models_to_compare:
        raise HTTPException(status_code=404, detail="No matching models found")

    return {
        "comparison": [
            {
                "model_id": m.id,
                "asset_name": m.asset_name,
                "total_lifecycle_cost": m.total_lifecycle_cost,
                "annual_equivalent_cost": m.annual_equivalent_cost,
            }
            for m in models_to_compare
        ],
        "lowest_cost_model": (
            min(models_to_compare, key=lambda m: m.total_lifecycle_cost).id if models_to_compare else None
        ),
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
