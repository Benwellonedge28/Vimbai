"""
Data Warehouse Service CRUD Operations

Dimension tables, fact tables, aggregate query records, and ETL jobs
persist as Neo4j nodes, caller-owned (X-User-Id) and Book-gated
(X-Book-ID). Aggregate queries resolve the fact table among the
caller's own Book-visible facts first.
"""

import json
from datetime import datetime, timezone
from typing import Dict, List, Optional

from data_warehouse_service.dependencies import book_id_var
from data_warehouse_service.models import AggregateQuery, DimensionTable, ETLJob, FactTable
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _as_dt(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, datetime):
        dt = value
    else:
        iso = value.iso_format() if hasattr(value, "iso_format") else str(value)
        if iso is None or iso == "None":
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _iso(dt: Optional[datetime]) -> Optional[str]:
    return dt.isoformat() if dt else None


def _j(v) -> str:
    return json.dumps(v or [])


def _jl(s, default=None):
    if s in (None, ""):
        return default if default is not None else []
    if isinstance(s, list):
        return s
    return json.loads(s)


def _jd(s):
    if s in (None, ""):
        return {}
    if isinstance(s, dict):
        return s
    return json.loads(s)


async def _create(session: AsyncSession, user_id: str, label: str, edge: str, props: str, params: Dict) -> Dict:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:{label} {{
{props}
    }})
    CREATE (u)-[:{edge}]->(x)
    RETURN x
    """
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return dict(records[0]["x"])


async def _list(session: AsyncSession, user_id: str, label: str, edge: str) -> List[Dict]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [dict(r["x"]) async for r in result]


# --- dimensions ---


def _dim_from_node(n: Dict) -> DimensionTable:
    return DimensionTable(
        id=n["id"],
        name=n.get("name", ""),
        columns=_jl(n.get("columns")),
        row_count=int(n.get("row_count") or 0),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_dimension(session: AsyncSession, user_id: str, d: DimensionTable) -> DimensionTable:
    props = """        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        columns: $columns,
        row_count: toInteger($row_count),
        created_at: datetime($created_at)"""
    params = {
        "id": d.id,
        "name": d.name,
        "columns": _j(d.columns),
        "row_count": d.row_count,
        "created_at": _iso(d.created_at),
    }
    return _dim_from_node(await _create(session, user_id, "WarehouseDimension", "OWNS_DIMENSION", props, params))


async def list_dimensions(session: AsyncSession, user_id: str) -> List[DimensionTable]:
    return [_dim_from_node(n) for n in await _list(session, user_id, "WarehouseDimension", "OWNS_DIMENSION")]


# --- facts ---


def _fact_from_node(n: Dict) -> FactTable:
    return FactTable(
        id=n["id"],
        name=n.get("name", ""),
        dimensions=_jl(n.get("dimensions")),
        measures=_jl(n.get("measures")),
        row_count=int(n.get("row_count") or 0),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_fact(session: AsyncSession, user_id: str, f: FactTable) -> FactTable:
    props = """        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        dimensions: $dimensions,
        measures: $measures,
        row_count: toInteger($row_count),
        created_at: datetime($created_at)"""
    params = {
        "id": f.id,
        "name": f.name,
        "dimensions": _j(f.dimensions),
        "measures": _j(f.measures),
        "row_count": f.row_count,
        "created_at": _iso(f.created_at),
    }
    return _fact_from_node(await _create(session, user_id, "WarehouseFact", "OWNS_FACT", props, params))


async def list_facts(session: AsyncSession, user_id: str) -> List[FactTable]:
    return [_fact_from_node(n) for n in await _list(session, user_id, "WarehouseFact", "OWNS_FACT")]


async def get_fact_by_name(session: AsyncSession, user_id: str, name: str) -> Optional[FactTable]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FACT]->(x:WarehouseFact)
    WHERE x.name = $name AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, name=name, user_id=user_id)
    record = await result.single()
    return _fact_from_node(dict(record["x"])) if record else None


# --- queries ---


def _query_from_node(n: Dict) -> AggregateQuery:
    return AggregateQuery(
        id=n["id"],
        fact_table=n.get("fact_table", ""),
        group_by=_jl(n.get("group_by")),
        measures=_jl(n.get("measures")),
        filters=_jd(n.get("filters")),
        results=_jl(n.get("results")),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_query(session: AsyncSession, user_id: str, q: AggregateQuery) -> AggregateQuery:
    props = """        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        fact_table: $fact_table,
        group_by: $group_by,
        measures: $measures,
        filters: $filters,
        results: $results,
        created_at: datetime($created_at)"""
    params = {
        "id": q.id,
        "fact_table": q.fact_table,
        "group_by": _j(q.group_by),
        "measures": _j(q.measures),
        "filters": json.dumps(q.filters or {}),
        "results": _j(q.results),
        "created_at": _iso(q.created_at),
    }
    return _query_from_node(await _create(session, user_id, "AggregateQueryRecord", "OWNS_QUERY", props, params))


# --- ETL jobs ---


def _etl_from_node(n: Dict) -> ETLJob:
    return ETLJob(
        id=n["id"],
        source=n.get("source", ""),
        target=n.get("target", ""),
        status=n.get("status", "pending"),
        rows_processed=int(n.get("rows_processed") or 0),
        started_at=_as_dt(n.get("started_at")),
        completed_at=_as_dt(n.get("completed_at")),
    )


async def create_etl_job(session: AsyncSession, user_id: str, j: ETLJob) -> ETLJob:
    props = """        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        source: $source,
        target: $target,
        status: $status,
        rows_processed: toInteger($rows_processed),
        started_at: datetime($started_at),
        completed_at: datetime($completed_at)"""
    params = {
        "id": j.id,
        "source": j.source,
        "target": j.target,
        "status": j.status,
        "rows_processed": j.rows_processed,
        "started_at": _iso(j.started_at),
        "completed_at": _iso(j.completed_at),
    }
    return _etl_from_node(await _create(session, user_id, "WarehouseETLJob", "OWNS_ETL_JOB", props, params))


async def list_etl_jobs(session: AsyncSession, user_id: str) -> List[ETLJob]:
    return [_etl_from_node(n) for n in await _list(session, user_id, "WarehouseETLJob", "OWNS_ETL_JOB")]
