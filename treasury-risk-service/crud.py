"""
Treasury Risk Service CRUD Operations

Exposures, VaR results, stress scenarios and stress results persist in
Neo4j, caller-owned (X-User-Id) and Book-gated (X-Book-ID, verified
upstream by the API gateway). Previously the four shared module-level
lists let any caller read every tenant's risk exposures, VaR history and
stress test results, and run stress tests against foreign scenarios.
"""

from typing import Dict, List, Optional

from neo4j import AsyncSession
from treasury_risk_service.dependencies import book_id_var
from treasury_risk_service.models import RiskExposure, StressTestResult, StressTestScenario, VaRResult

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _as_dt(value) -> Optional[object]:
    if value is None:
        return None
    if hasattr(value, "iso_format"):  # Temporal
        iso = value.iso_format()
        if iso is None or iso == "None":
            return None
        from datetime import datetime, timezone

        dt = datetime.fromisoformat(iso)
        return dt if dt.tzinfo else dt.replace(tzinfo=timezone.utc)
    return value


def _iso(dt) -> Optional[str]:
    if dt is None:
        return None
    if hasattr(dt, "iso_format"):  # Temporal
        return dt.iso_format()
    if getattr(dt, "tzinfo", None) is None:
        from datetime import timezone

        dt = dt.replace(tzinfo=timezone.utc)
    return dt.isoformat()


# --- exposures ---


def _exposure_from_node(n: Dict) -> RiskExposure:
    return RiskExposure(
        id=n["id"],
        exposure_type=n.get("exposure_type", ""),
        currency=n.get("currency", "USD"),
        notional_amount=float(n.get("notional_amount", 0.0)),
        description=n.get("description", ""),
        created_at=_as_dt(n.get("created_at")),
    )


async def create_exposure(session: AsyncSession, user_id: str, e: RiskExposure) -> RiskExposure:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:RiskExposure {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        exposure_type: $exposure_type,
        currency: $currency,
        notional_amount: toFloat($notional_amount),
        description: $description,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_EXPOSURE]->(x)
    RETURN x
    """
    params = {
        "id": e.id,
        "exposure_type": e.exposure_type,
        "currency": e.currency,
        "notional_amount": float(e.notional_amount),
        "description": e.description,
        "created_at": _iso(e.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _exposure_from_node(dict(rec["x"]))


async def list_exposures(session: AsyncSession, user_id: str) -> List[RiskExposure]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_EXPOSURE]->(x:RiskExposure)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_exposure_from_node(dict(rec["x"])) async for rec in result]


# --- VaR results ---


def _var_from_node(n: Dict) -> VaRResult:
    return VaRResult(
        id=n["id"],
        portfolio_value=float(n.get("portfolio_value", 0.0)),
        confidence_level=float(n.get("confidence_level", 0.95)),
        holding_period_days=int(n.get("holding_period_days", 1)),
        var_amount=float(n.get("var_amount", 0.0)),
        var_pct=float(n.get("var_pct", 0.0)),
        method=n.get("method", "parametric"),
        calculated_at=_as_dt(n.get("calculated_at")),
    )


async def create_var(session: AsyncSession, user_id: str, v: VaRResult) -> VaRResult:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:VaRResult {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        portfolio_value: toFloat($portfolio_value),
        confidence_level: toFloat($confidence_level),
        holding_period_days: toFloat($holding_period_days),
        var_amount: toFloat($var_amount),
        var_pct: toFloat($var_pct),
        method: $method,
        calculated_at: datetime($calculated_at)
    })
    CREATE (u)-[:OWNS_VAR]->(x)
    RETURN x
    """
    params = {
        "id": v.id,
        "portfolio_value": float(v.portfolio_value),
        "confidence_level": float(v.confidence_level),
        "holding_period_days": float(v.holding_period_days),
        "var_amount": float(v.var_amount),
        "var_pct": float(v.var_pct),
        "method": v.method,
        "calculated_at": _iso(v.calculated_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _var_from_node(dict(rec["x"]))


async def list_var_results(session: AsyncSession, user_id: str) -> List[VaRResult]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_VAR]->(x:VaRResult)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_var_from_node(dict(rec["x"])) async for rec in result]


# --- scenarios ---


def _scenario_from_node(n: Dict) -> StressTestScenario:
    return StressTestScenario(
        id=n["id"],
        name=n.get("name", ""),
        description=n.get("description", ""),
        shock_type=n.get("shock_type", ""),
        shock_magnitude=float(n.get("shock_magnitude", 0.0)),
        portfolio_impact=float(n.get("portfolio_impact", 0.0)),
        created_at=_as_dt(n.get("created_at")),
    )


async def create_scenario(session: AsyncSession, user_id: str, s: StressTestScenario) -> StressTestScenario:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:StressTestScenario {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        description: $description,
        shock_type: $shock_type,
        shock_magnitude: toFloat($shock_magnitude),
        portfolio_impact: toFloat($portfolio_impact),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_SCENARIO]->(x)
    RETURN x
    """
    params = {
        "id": s.id,
        "name": s.name,
        "description": s.description,
        "shock_type": s.shock_type,
        "shock_magnitude": float(s.shock_magnitude),
        "portfolio_impact": float(s.portfolio_impact),
        "created_at": _iso(s.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _scenario_from_node(dict(rec["x"]))


async def get_scenario(session: AsyncSession, user_id: str, scenario_id: str) -> Optional[StressTestScenario]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SCENARIO]->(x:StressTestScenario)
    WHERE x.id = $scenario_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, scenario_id=scenario_id, user_id=user_id)
    rec = await result.single()
    return _scenario_from_node(dict(rec["x"])) if rec else None


async def list_scenarios(session: AsyncSession, user_id: str) -> List[StressTestScenario]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SCENARIO]->(x:StressTestScenario)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_scenario_from_node(dict(rec["x"])) async for rec in result]


# --- stress results ---


def _stress_from_node(n: Dict) -> StressTestResult:
    return StressTestResult(
        id=n["id"],
        scenario_id=n.get("scenario_id", ""),
        portfolio_value_before=float(n.get("portfolio_value_before", 0.0)),
        portfolio_value_after=float(n.get("portfolio_value_after", 0.0)),
        impact=float(n.get("impact", 0.0)),
        impact_pct=float(n.get("impact_pct", 0.0)),
        tested_at=_as_dt(n.get("tested_at")),
    )


async def create_stress_result(session: AsyncSession, user_id: str, r: StressTestResult) -> StressTestResult:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:StressTestResult {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        scenario_id: $scenario_id,
        portfolio_value_before: toFloat($portfolio_value_before),
        portfolio_value_after: toFloat($portfolio_value_after),
        impact: toFloat($impact),
        impact_pct: toFloat($impact_pct),
        tested_at: datetime($tested_at)
    })
    CREATE (u)-[:OWNS_STRESS_RESULT]->(x)
    RETURN x
    """
    params = {
        "id": r.id,
        "scenario_id": r.scenario_id,
        "portfolio_value_before": float(r.portfolio_value_before),
        "portfolio_value_after": float(r.portfolio_value_after),
        "impact": float(r.impact),
        "impact_pct": float(r.impact_pct),
        "tested_at": _iso(r.tested_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    rec = await result.single()
    return _stress_from_node(dict(rec["x"]))


async def list_stress_results(session: AsyncSession, user_id: str) -> List[StressTestResult]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_STRESS_RESULT]->(x:StressTestResult)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_stress_from_node(dict(rec["x"])) async for rec in result]
