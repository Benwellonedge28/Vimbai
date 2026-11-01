"""
Rolling Forecast Service CRUD.

Forecasts move from the in-memory list to Neo4j: :RollingForecast nodes via
:OWNS_FORECAST edges, book_id stamped and Book-gated, so one caller can no
longer read or roll another's forecast by guessing an id.

Periods stay a JSON prop on the node. That keeps the variance arithmetic in
Python where it is already tested, and preserves the original quirks: a
roll appends whatever periods it is given without trimming the horizon, and
updating a period that does not exist reports a null variance instead of
failing.
"""

import json
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from rolling_forecast_service.dependencies import book_id_var
from rolling_forecast_service.exceptions import NotFoundError
from rolling_forecast_service.models import ForecastPeriod, RollingForecast

LABEL = "RollingForecast"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _coerce_dt(value: Any) -> datetime:
    if isinstance(value, datetime):
        return value
    if value is None:
        return datetime.now(timezone.utc)
    if hasattr(value, "iso_format"):
        try:
            return datetime.fromisoformat(value.iso_format())
        except (TypeError, ValueError):
            return datetime.now(timezone.utc)
    if isinstance(value, str):
        try:
            return datetime.fromisoformat(value)
        except ValueError:
            return datetime.now(timezone.utc)
    return datetime.now(timezone.utc)


def _periods_from_node(raw: Any) -> List[ForecastPeriod]:
    if isinstance(raw, str):
        try:
            raw = json.loads(raw)
        except ValueError:
            raw = []
    return [ForecastPeriod(**p) for p in (raw or [])]


def _forecast_from_node(n: Dict[str, Any]) -> RollingForecast:
    return RollingForecast(
        id=n["id"],
        name=n.get("name", ""),
        metric=n.get("metric", ""),
        frequency=n.get("frequency", "monthly"),
        horizon_periods=int(n.get("horizon_periods", 12)),
        periods=_periods_from_node(n.get("periods", "[]")),
        current_period=n.get("current_period", ""),
        last_updated=_coerce_dt(n.get("last_updated")),
        created_at=_coerce_dt(n.get("created_at")),
    )


async def create_forecast(session: AsyncSession, user_id: str, forecast: RollingForecast) -> RollingForecast:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:{LABEL} {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        metric: $metric,
        frequency: $frequency,
        horizon_periods: $horizon_periods,
        periods: $periods,
        current_period: $current_period,
        last_updated: datetime($last_updated),
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_FORECAST]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        id=forecast.id,
        user_id=user_id,
        name=forecast.name,
        metric=forecast.metric,
        frequency=forecast.frequency,
        horizon_periods=forecast.horizon_periods,
        periods=json.dumps([p.model_dump(mode="json") for p in forecast.periods]),
        current_period=forecast.current_period,
        last_updated=forecast.last_updated.isoformat(),
        created_at=forecast.created_at.isoformat(),
    )
    return forecast


async def list_forecasts(session: AsyncSession, user_id: str, metric: Optional[str] = None) -> List[RollingForecast]:
    where = ["x.user_id = $user_id"]
    params: Dict[str, Any] = {"user_id": user_id}
    where.append("($book_id IS NULL OR x.book_id = $book_id)")
    if metric:
        where.append("x.metric = $metric")
        params["metric"] = metric
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FORECAST]->(x:{LABEL})
    WHERE {' AND '.join(where)}
    RETURN x
    ORDER BY x.created_at DESC
    """
    result = await _run(session, query, params)
    out: List[RollingForecast] = []
    async for record in result:
        out.append(_forecast_from_node(dict(record["x"])))
    return out


async def get_forecast(session: AsyncSession, user_id: str, forecast_id: str) -> RollingForecast:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FORECAST]->(x:{LABEL} {{id: $forecast_id}})
    WHERE ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, forecast_id=forecast_id)
    record = await result.single()
    if not record:
        raise NotFoundError("Forecast not found")
    return _forecast_from_node(dict(record["x"]))


async def save_periods(
    session: AsyncSession,
    user_id: str,
    forecast: RollingForecast,
    periods: List[ForecastPeriod],
) -> RollingForecast:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FORECAST]->(x:{LABEL} {{id: $forecast_id}})
    WHERE ($book_id IS NULL OR x.book_id = $book_id)
    SET x.periods = $periods,
        x.current_period = $current_period,
        x.last_updated = datetime($last_updated)
    RETURN x
    """
    forecast.periods = periods
    forecast.last_updated = datetime.now(timezone.utc)
    await _run(
        session,
        query,
        user_id=user_id,
        forecast_id=forecast.id,
        periods=json.dumps([p.model_dump(mode="json") for p in periods]),
        current_period=forecast.current_period,
        last_updated=forecast.last_updated.isoformat(),
    )
    return forecast
