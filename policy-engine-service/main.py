"""Vimbai Policy Engine Service - Business rule and policy enforcement. Port: 8363"""
import os, uuid
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Optional
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "policy-engine-service"
PORT = int(os.getenv("PORT", "8363"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Policy Engine Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="policy-engine-service", instrument_app=app)
except ImportError:
    TRACER = None

class PolicyAction(str, Enum):
    ALLOW = "allow"; DENY = "deny"; WARN = "warn"; REQUIRE_APPROVAL = "require_approval"

class PolicyRule(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    description: str = ""
    resource_type: str  # transaction, invoice, payment, report
    condition_field: str  # e.g., "amount", "currency", "category"
    condition_operator: str = ">"  # >, <, ==, >=, <=, contains
    condition_value: Any
    action: PolicyAction = PolicyAction.WARN
    message: str = ""
    enabled: bool = True

class PolicyEvaluation(BaseModel):
    rule_id: str
    rule_name: str
    action: PolicyAction
    message: str
    triggered: bool = False

_rules: Dict[str, List[PolicyRule]] = defaultdict(list)

def evaluate_rule(rule: PolicyRule, data: Dict[str, Any]) -> PolicyEvaluation:
    val = data.get(rule.condition_field)
    triggered = False
    if val is not None:
        try:
            if rule.condition_operator == ">": triggered = val > rule.condition_value
            elif rule.condition_operator == "<": triggered = val < rule.condition_value
            elif rule.condition_operator == "==": triggered = val == rule.condition_value
            elif rule.condition_operator == ">=": triggered = val >= rule.condition_value
            elif rule.condition_operator == "<=": triggered = val <= rule.condition_value
            elif rule.condition_operator == "contains": triggered = str(rule.condition_value) in str(val)
        except TypeError:
            pass
    return PolicyEvaluation(rule_id=rule.id, rule_name=rule.name, action=rule.action, message=rule.message, triggered=triggered)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/rules/{company_id}")
async def create_rule(company_id: str, rule: PolicyRule):
    _rules[company_id].append(rule)
    return {"id": rule.id, "name": rule.name, "action": rule.action.value}

@app.get("/rules/{company_id}")
async def get_rules(company_id: str):
    return {"company_id": company_id, "rules": _rules.get(company_id, []), "total": len(_rules.get(company_id, []))}

@app.post("/evaluate/{company_id}")
async def evaluate(company_id: str, resource_type: str, data: Dict[str, Any]):
    rules = [r for r in _rules.get(company_id, []) if r.enabled and r.resource_type == resource_type]
    results = [evaluate_rule(r, data) for r in rules]
    triggered = [r for r in results if r.triggered]
    has_block = any(r.action == PolicyAction.DENY for r in triggered)
    return {"resource_type": resource_type, "evaluations": results, "triggered_count": len(triggered), "blocked": has_block, "allowed": not has_block}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
