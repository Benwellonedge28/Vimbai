"""Vimbai Policy Engine Service - business rules and governance enforcement. Port: 8306

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "policy_engine_service" not in _sys.modules or not hasattr(_sys.modules.get("policy_engine_service"), "__path__"):
    _spec = importlib.util.spec_from_file_location("policy_engine_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["policy_engine_service"] = _pkg
    _sys.modules["policy_engine_service"].__path__ = [_HERE]

import os
from typing import Any, Dict

import structlog
from fastapi import Depends, FastAPI, Request
from neo4j import AsyncSession
from policy_engine_service import crud, models
from policy_engine_service.dependencies import book_id_var, get_db_session, get_user_id

SERVICE_NAME = "policy-engine-service"
PORT = int(os.getenv("PORT", "8306"))
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
app = FastAPI(title="Vimbai Policy Engine Service", version="2.0.0", docs_url="/docs")
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


def evaluate_rule(rule: models.PolicyRule, data: Dict[str, Any]) -> models.PolicyEvaluation:
    val = data.get(rule.condition_field)
    triggered = False
    if val is not None:
        try:
            if rule.condition_operator == ">":
                triggered = val > rule.condition_value
            elif rule.condition_operator == "<":
                triggered = val < rule.condition_value
            elif rule.condition_operator == "==":
                triggered = val == rule.condition_value
            elif rule.condition_operator == ">=":
                triggered = val >= rule.condition_value
            elif rule.condition_operator == "<=":
                triggered = val <= rule.condition_value
            elif rule.condition_operator == "contains":
                triggered = str(rule.condition_value) in str(val)
        except TypeError:
            pass
    return models.PolicyEvaluation(
        rule_id=rule.id, rule_name=rule.name, action=rule.action, message=rule.message, triggered=triggered
    )


@app.get("/")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME}


@app.post("/rules/{company_id}")
async def create_rule(
    company_id: str,
    rule: models.PolicyRule,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    stored = await crud.create_rule(db_session, user_id, company_id, rule)
    logger.info("policy_rule_created", company_id=company_id, rule=stored.name)
    return {"id": stored.id, "name": stored.name, "action": stored.action.value}


@app.get("/rules/{company_id}")
async def get_rules(
    company_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    return await crud.get_rules(db_session, user_id, company_id)


@app.post("/evaluate/{company_id}")
async def evaluate(
    company_id: str,
    resource_type: str,
    data: Dict[str, Any],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    rules = await crud.get_rules_for_evaluation(db_session, user_id, company_id, resource_type)
    results = [evaluate_rule(r, data) for r in rules]
    triggered = [r for r in results if r.triggered]
    has_block = any(r.action == models.PolicyAction.DENY for r in triggered)
    return {
        "resource_type": resource_type,
        "evaluations": results,
        "triggered_count": len(triggered),
        "blocked": has_block,
        "allowed": not has_block,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
