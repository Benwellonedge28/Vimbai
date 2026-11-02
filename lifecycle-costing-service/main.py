"""
Vimbai Lifecycle Costing Service
Calculates total cost of ownership across asset lifecycle phases.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "lifecycle_costing_service" not in _sys.modules or not hasattr(
    _sys.modules.get("lifecycle_costing_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("lifecycle_costing_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["lifecycle_costing_service"] = _pkg
    _sys.modules["lifecycle_costing_service"].__path__ = [_HERE]

import os
from typing import Any, Dict, List

import structlog
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from lifecycle_costing_service import crud
from lifecycle_costing_service.dependencies import book_id_var, get_db_session, get_user_id
from lifecycle_costing_service.models import LifecycleCostModel, LifecyclePhase
from neo4j import AsyncSession

SERVICE_NAME = "lifecycle-costing-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8182"))

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


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


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
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
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
    stored = await crud.create(session, user_id, model)
    logger.info("Lifecycle cost model created", model_id=stored.id, asset=asset_name, total=total)
    return stored


@app.get("/models", response_model=List[LifecycleCostModel])
async def list_models(
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """List all lifecycle cost models."""
    return await crud.list_all(session, user_id, LifecycleCostModel)


@app.get("/models/{model_id}", response_model=LifecycleCostModel)
async def get_model(
    model_id: str,
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Get a specific cost model."""
    model = await crud.find(session, user_id, LifecycleCostModel, model_id)
    if not model:
        raise HTTPException(status_code=404, detail="Cost model not found")
    return model


@app.post("/compare")
async def compare_models(
    model_ids: List[str],
    session: AsyncSession = Depends(get_db_session),
    user_id: str = Depends(get_user_id),
):
    """Compare multiple lifecycle cost models."""
    owned = await crud.list_all(session, user_id, LifecycleCostModel)
    models_to_compare = [m for m in owned if m.id in model_ids]
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
