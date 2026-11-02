"""
Vimbai Basis Apportionment Service
Apportioning overhead costs across cost centres by the classical bases.

Rules and results live in Neo4j, scoped to the caller and the Book. The file may be
imported bare (bracket mounts, uvicorn main:app), so it bootstraps its own package
alias first.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "basis_apportionment_service" not in _sys.modules or not hasattr(
    _sys.modules.get("basis_apportionment_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("basis_apportionment_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["basis_apportionment_service"] = _pkg
    _sys.modules["basis_apportionment_service"].__path__ = [_HERE]

import os
from enum import Enum
from typing import Any, Dict, List, Optional

import structlog
from basis_apportionment_service import crud
from basis_apportionment_service.dependencies import book_id_var, get_db_session, get_user_id
from fastapi import Depends, FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from neo4j import AsyncSession

SERVICE_NAME = "basis-apportionment-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8070"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")

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

app = FastAPI(title="Vimbai Basis of Apportionment Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


class ApportionmentBasis(str, Enum):
    FLOOR_AREA = "floor_area"
    VOLUME = "volume"
    COST = "cost"
    NET_BOOK_VALUE = "net_book_value"
    REPLACEMENT_VALUE = "replacement_value"
    NUMBER_OF_REQUISITIONS = "number_of_requisitions"
    VALUE_OF_REQUISITIONS = "value_of_requisitions"
    NUMBER_OF_PERSONNEL = "number_of_personnel"
    MACHINE_HOURS = "machine_hours"
    LABOR_HOURS = "labor_hours"
    UNITS = "units"


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Basis of apportionment"}


@app.post("/rules/create")
async def create_apportionment_rule(
    cost_type: str,
    basis: str,
    description: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create an apportionment rule."""
    return await crud.create_rule(db_session, user_id, cost_type=cost_type, basis=basis, description=description)


@app.get("/rules")
async def list_apportionment_rules(
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List all apportionment rules."""
    return {"rules": await crud.list_rules(db_session, user_id)}


@app.post("/apportion/floor-area")
async def apportion_by_floor_area(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, floor_area}]
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Apportion costs by floor area."""
    return await crud.apportion(
        db_session,
        user_id,
        cost_type=cost_type,
        total_cost=total_cost,
        basis="floor_area",
        basis_key="floor_area",
        cost_centre_data=cost_centre_data,
    )


@app.post("/apportion/volume")
async def apportion_by_volume(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, volume}]
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Apportion costs by volume (space occupied)."""
    return await crud.apportion(
        db_session,
        user_id,
        cost_type=cost_type,
        total_cost=total_cost,
        basis="volume",
        basis_key="volume",
        cost_centre_data=cost_centre_data,
    )


@app.post("/apportion/cost")
async def apportion_by_cost(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, asset_cost}]
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Apportion costs by cost/value of assets."""
    return await crud.apportion(
        db_session,
        user_id,
        cost_type=cost_type,
        total_cost=total_cost,
        basis="cost",
        basis_key="asset_cost",
        cost_centre_data=cost_centre_data,
    )


@app.post("/apportion/net-book-value")
async def apportion_by_net_book_value(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, net_book_value}]
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Apportion costs by net book value of machinery."""
    return await crud.apportion(
        db_session,
        user_id,
        cost_type=cost_type,
        total_cost=total_cost,
        basis="net_book_value",
        basis_key="net_book_value",
        cost_centre_data=cost_centre_data,
    )


@app.post("/apportion/replacement-value")
async def apportion_by_replacement_value(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, replacement_value}]
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Apportion costs by replacement value of assets."""
    return await crud.apportion(
        db_session,
        user_id,
        cost_type=cost_type,
        total_cost=total_cost,
        basis="replacement_value",
        basis_key="replacement_value",
        cost_centre_data=cost_centre_data,
    )


@app.post("/apportion/personnel")
async def apportion_by_personnel(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, number_of_personnel}]
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Apportion costs by number of personnel."""
    return await crud.apportion(
        db_session,
        user_id,
        cost_type=cost_type,
        total_cost=total_cost,
        basis="number_of_personnel",
        basis_key="number_of_personnel",
        cost_centre_data=cost_centre_data,
    )


@app.post("/apportion/requisitions")
async def apportion_by_requisitions(
    cost_type: str,
    total_cost: float,
    cost_centre_data: List[Dict[str, Any]],  # [{cost_centre_id, cost_centre_name, number_of_requisitions}]
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Apportion costs by number of requisitions."""
    return await crud.apportion(
        db_session,
        user_id,
        cost_type=cost_type,
        total_cost=total_cost,
        basis="number_of_requisitions",
        basis_key="number_of_requisitions",
        cost_centre_data=cost_centre_data,
    )


@app.get("/results")
async def list_apportionment_results(
    cost_type: Optional[str] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List apportionment results."""
    return {"results": await crud.list_results(db_session, user_id, cost_type)}


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
