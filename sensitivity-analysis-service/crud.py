"""
Sensitivity Analysis Service CRUD Operations

The what-if engine stays a pure computation (estimate_impact linear
model, elasticity, most-sensitive-variable ranking, unchanged
semantics); completed analyses are persisted as :SensitivityAnalysis
nodes via :OWNS_ANALYSIS edges with the results list stored as a JSON
prop. Every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List

from neo4j import AsyncSession
from sensitivity_analysis_service.dependencies import book_id_var
from sensitivity_analysis_service.models import AnalysisResponse

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _as_dt(value) -> datetime:
    if value is None:
        return _now()
    if isinstance(value, datetime):
        dt = value
    else:
        iso = value.iso_format() if hasattr(value, "iso_format") else str(value)
        if iso is None or iso == "None":
            return _now()
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _analysis_from_node(n: Dict[str, Any], user_id: str) -> AnalysisResponse:
    return AnalysisResponse(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        target_metric=n["target_metric"],
        base_target_value=float(n.get("base_target_value", 0)),
        results=json.loads(n.get("results_json") or "[]"),
        most_sensitive_variable=n.get("most_sensitive_variable", ""),
        created_at=_as_dt(n.get("created_at")),
    )


async def store_analysis(session: AsyncSession, user_id: str, analysis: AnalysisResponse) -> None:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:SensitivityAnalysis {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        target_metric: $target_metric,
        base_target_value: toFloat($base_target_value),
        results_json: $results_json,
        most_sensitive_variable: $most_sensitive_variable,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_ANALYSIS]->(x)
    RETURN x
    """
    params = {
        "id": analysis.id,
        "user_id": user_id,
        "company_id": analysis.company_id,
        "target_metric": analysis.target_metric,
        "base_target_value": analysis.base_target_value,
        "results_json": json.dumps([r.model_dump() for r in analysis.results]),
        "most_sensitive_variable": analysis.most_sensitive_variable,
        "created_at": _now().isoformat(),
    }
    await _run(session, query, params)


async def get_analyses(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ANALYSIS]->(x:SensitivityAnalysis {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    analyses = [_analysis_from_node(dict(r["x"]), user_id) async for r in result]
    return {"company_id": company_id, "analyses": analyses, "total": len(analyses)}
