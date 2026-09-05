"""
Job Costing Service CRUD Operations

Neo4j-backed persistence for jobs and their cost entries. All records are
stamped with book_id; every read applies the Book filter
`WHERE ($book_id IS NULL OR x.book_id = $book_id)`.

Cost entries are child nodes (:JobCostEntry) linked to their job via
:HAS_COST edges. Job cost aggregates are recomputed server side on each
add_cost and stored on the job node (single source of truth remains the
entries; aggregates are derived).
"""

import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from job_costing_service.dependencies import book_id_var
from job_costing_service.exceptions import NotFoundError
from job_costing_service.models import COST_TYPES, Job, JobCostEntry, JobCostEntryCreate, JobCreate
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _dt(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, str):
        return datetime.fromisoformat(value)
    return datetime.fromisoformat(value.iso_format())


def _job_from_node(n: Dict[str, Any], user_id: str) -> Job:
    return Job(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        job_name=n["job_name"],
        customer=n.get("customer", ""),
        contract_value=float(n.get("contract_value", 0)),
        status=n.get("status", "active"),
        start_date=_dt(n.get("start_date")),
        end_date=_dt(n.get("end_date")),
        materials_cost=float(n.get("materials_cost", 0)),
        labor_cost=float(n.get("labor_cost", 0)),
        overhead_cost=float(n.get("overhead_cost", 0)),
        subcontractor_cost=float(n.get("subcontractor_cost", 0)),
        total_cost=float(n.get("total_cost", 0)),
        gross_profit=float(n.get("gross_profit", 0)),
        gross_margin=float(n.get("gross_margin", 0)),
        created_at=_dt(n.get("created_at")),
    )


def _entry_from_node(n: Dict[str, Any], user_id: str) -> JobCostEntry:
    return JobCostEntry(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        job_id=n.get("job_id", ""),
        cost_type=n["cost_type"],
        amount=float(n["amount"]),
        date=_dt(n.get("date")),
        description=n.get("description", ""),
        created_at=_dt(n.get("created_at")),
    )


async def create_job(session: AsyncSession, user_id: str, payload: JobCreate) -> Job:
    data = payload.model_dump()
    if data.get("start_date") is None:
        data.pop("start_date")
    job = Job(**data)
    job.start_date = payload.start_date or _now()
    job_id = str(uuid.uuid4())
    now = _now()
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:Job {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        job_name: $job_name,
        customer: $customer,
        contract_value: toFloat($contract_value),
        status: $status,
        start_date: datetime($start_date),
        end_date: datetime($end_date),
        materials_cost: toFloat(0),
        labor_cost: toFloat(0),
        overhead_cost: toFloat(0),
        subcontractor_cost: toFloat(0),
        total_cost: toFloat(0),
        gross_profit: toFloat(0),
        gross_margin: toFloat(0),
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_JOB]->(x)
    RETURN x
    """
    params = {
        "id": job_id,
        "user_id": user_id,
        "company_id": job.company_id,
        "job_name": job.job_name,
        "customer": job.customer,
        "contract_value": job.contract_value,
        "status": job.status,
        "start_date": job.start_date.isoformat(),
        "end_date": (job.end_date or now).isoformat(),
        "created_at": now.isoformat(),
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _job_from_node(dict(records[0]["x"]), user_id)


async def get_jobs(session: AsyncSession, user_id: str, company_id: str, status: Optional[str] = None) -> List[Job]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_JOB]->(x:Job {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    jobs = [_job_from_node(dict(r["x"]), user_id) async for r in result]
    if status:
        jobs = [j for j in jobs if j.status == status]
    return jobs


async def get_job(session: AsyncSession, user_id: str, job_id: str) -> Optional[Job]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_JOB]->(x:Job {{id: $job_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, job_id=job_id)
    records = [r async for r in result]
    if not records:
        return None
    return _job_from_node(dict(records[0]["x"]), user_id)


async def get_job_entries(session: AsyncSession, user_id: str, job_id: str) -> List[JobCostEntry]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_JOB]->(j:Job {{id: $job_id}})
    {BOOK_FILTER}
    MATCH (j)-[:HAS_COST]->(x:JobCostEntry)
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, job_id=job_id)
    return [_entry_from_node(dict(r["x"]), user_id) async for r in result]


async def add_cost(session: AsyncSession, user_id: str, job_id: str, payload: JobCostEntryCreate) -> Dict[str, Any]:
    job = await get_job(session, user_id, job_id)
    if job is None:
        raise NotFoundError(detail="Job not found")

    # Recompute aggregates server side from the new entry (only the four
    # known cost types roll up; unknown types are stored but not aggregated,
    # matching the original service behavior).
    if payload.cost_type in COST_TYPES:
        field = f"{payload.cost_type}_cost"
        setattr(job, field, getattr(job, field) + payload.amount)
        job.total_cost = job.materials_cost + job.labor_cost + job.overhead_cost + job.subcontractor_cost
        job.gross_profit = job.contract_value - job.total_cost
        job.gross_margin = (job.gross_profit / max(1, job.contract_value)) * 100

    entry_id = str(uuid.uuid4())
    now = _now()
    entry_query = """
    MATCH (j:Job {id: $job_id})
    CREATE (x:JobCostEntry {
        id: $entry_id,
        user_id: $user_id,
        book_id: $book_id,
        job_id: $job_id,
        cost_type: $cost_type,
        amount: toFloat($amount),
        date: datetime($date),
        description: $description,
        created_at: datetime($created_at)
    })
    CREATE (j)-[:HAS_COST]->(x)
    RETURN x
    """
    entry_params = {
        "job_id": job_id,
        "entry_id": entry_id,
        "user_id": user_id,
        "cost_type": payload.cost_type,
        "amount": payload.amount,
        "date": (payload.date or now).isoformat(),
        "description": payload.description,
        "created_at": now.isoformat(),
    }
    await _run(session, entry_query, entry_params)

    # Persist the recomputed aggregates on the job node
    update_query = """
    MATCH (x:Job {id: $job_id})
    SET x.materials_cost = toFloat($materials_cost),
        x.labor_cost = toFloat($labor_cost),
        x.overhead_cost = toFloat($overhead_cost),
        x.subcontractor_cost = toFloat($subcontractor_cost),
        x.total_cost = toFloat($total_cost),
        x.gross_profit = toFloat($gross_profit),
        x.gross_margin = toFloat($gross_margin)
    RETURN x
    """
    update_params = {
        "job_id": job_id,
        "materials_cost": job.materials_cost,
        "labor_cost": job.labor_cost,
        "overhead_cost": job.overhead_cost,
        "subcontractor_cost": job.subcontractor_cost,
        "total_cost": job.total_cost,
        "gross_profit": job.gross_profit,
        "gross_margin": job.gross_margin,
    }
    await _run(session, update_query, update_params)

    return {
        "job_id": job_id,
        "total_cost": job.total_cost,
        "gross_profit": job.gross_profit,
        "margin": job.gross_margin,
    }


async def job_profitability(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    jobs = await get_jobs(session, user_id, company_id)
    completed = [j for j in jobs if j.status in ("completed", "closed")]
    return {
        "company_id": company_id,
        "total_jobs": len(jobs),
        "completed": len(completed),
        "total_contract_value": sum(j.contract_value for j in jobs),
        "total_cost": sum(j.total_cost for j in jobs),
        "total_profit": sum(j.gross_profit for j in jobs),
        "avg_margin": (sum(j.gross_margin for j in jobs) / max(1, len(jobs))),
    }
