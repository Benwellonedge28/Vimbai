"""
SOX Compliance Service CRUD Operations

Controls, control tests, and deficiencies persist as Neo4j nodes,
caller-owned (X-User-Id) and Book-gated (X-Book-ID). Recording a test
against a control checks the caller's own Book-visible control first;
deficiency updates are scoped the same way.
"""

from datetime import datetime, timezone
from typing import Dict, List, Optional

from neo4j import AsyncSession
from sox_compliance_service.dependencies import book_id_var
from sox_compliance_service.models import Control, ControlTest, Deficiency

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


def _f(v, default=0):
    return v if v is not None else default


# --- controls ---


def _control_from_node(n: Dict) -> Control:
    return Control(
        id=n["id"],
        control_id_ref=n.get("control_id_ref", ""),
        description=n.get("description", ""),
        control_type=n.get("control_type", ""),
        control_nature=n.get("control_nature", ""),
        frequency=n.get("frequency", ""),
        owner=n.get("owner", ""),
        process=n.get("process", ""),
        risk_level=n.get("risk_level", "medium"),
        status=n.get("status", "active"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_control(session: AsyncSession, user_id: str, c: Control) -> Control:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:SOXControl {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        control_id_ref: $control_id_ref,
        description: $description,
        control_type: $control_type,
        control_nature: $control_nature,
        frequency: $frequency,
        owner: $owner,
        process: $process,
        risk_level: $risk_level,
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CONTROL]->(x)
    RETURN x
    """
    params = {
        "id": c.id,
        "control_id_ref": c.control_id_ref,
        "description": c.description,
        "control_type": c.control_type,
        "control_nature": c.control_nature,
        "frequency": c.frequency,
        "owner": c.owner,
        "process": c.process,
        "risk_level": c.risk_level,
        "status": c.status,
        "created_at": _iso(c.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _control_from_node(dict(records[0]["x"]))


async def list_controls(session: AsyncSession, user_id: str) -> List[Control]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CONTROL]->(x:SOXControl)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_control_from_node(dict(r["x"])) async for r in result]


async def get_control(session: AsyncSession, user_id: str, control_id: str) -> Optional[Control]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CONTROL]->(x:SOXControl)
    WHERE x.id = $control_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, control_id=control_id, user_id=user_id)
    record = await result.single()
    return _control_from_node(dict(record["x"])) if record else None


# --- control tests ---


def _test_from_node(n: Dict) -> ControlTest:
    return ControlTest(
        id=n["id"],
        control_id=n.get("control_id", ""),
        test_period=n.get("test_period", ""),
        tester=n.get("tester", ""),
        sample_size=int(_f(n.get("sample_size"), 25)),
        exceptions_found=int(_f(n.get("exceptions_found"), 0)),
        result=n.get("result", "pass"),
        test_date=_as_dt(n.get("test_date")) or datetime.now(timezone.utc),
        notes=n.get("notes", ""),
    )


async def create_test(session: AsyncSession, user_id: str, t: ControlTest) -> ControlTest:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:SOXControlTest {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        control_id: $control_id,
        test_period: $test_period,
        tester: $tester,
        sample_size: toInteger($sample_size),
        exceptions_found: toInteger($exceptions_found),
        result: $result,
        test_date: datetime($test_date),
        notes: $notes
    })
    CREATE (u)-[:OWNS_TEST]->(x)
    RETURN x
    """
    params = {
        "id": t.id,
        "control_id": t.control_id,
        "test_period": t.test_period,
        "tester": t.tester,
        "sample_size": t.sample_size,
        "exceptions_found": t.exceptions_found,
        "result": t.result,
        "test_date": _iso(t.test_date),
        "notes": t.notes,
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _test_from_node(dict(records[0]["x"]))


async def list_tests(session: AsyncSession, user_id: str, control_id: str) -> List[ControlTest]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TEST]->(x:SOXControlTest)
    WHERE x.control_id = $control_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, control_id=control_id, user_id=user_id)
    return [_test_from_node(dict(r["x"])) async for r in result]


# --- deficiencies ---


def _def_from_node(n: Dict) -> Deficiency:
    return Deficiency(
        id=n["id"],
        control_id=n.get("control_id", ""),
        severity=n.get("severity", ""),
        description=n.get("description", ""),
        remediation_plan=n.get("remediation_plan", ""),
        remediation_owner=n.get("remediation_owner", ""),
        status=n.get("status", "open"),
        identified_date=_as_dt(n.get("identified_date")) or datetime.now(timezone.utc),
        remediated_date=_as_dt(n.get("remediated_date")),
    )


async def create_deficiency(session: AsyncSession, user_id: str, d: Deficiency) -> Deficiency:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:SOXDeficiency {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        control_id: $control_id,
        severity: $severity,
        description: $description,
        remediation_plan: $remediation_plan,
        remediation_owner: $remediation_owner,
        status: $status,
        identified_date: datetime($identified_date),
        remediated_date: datetime($remediated_date)
    })
    CREATE (u)-[:OWNS_DEFICIENCY]->(x)
    RETURN x
    """
    params = {
        "id": d.id,
        "control_id": d.control_id,
        "severity": d.severity,
        "description": d.description,
        "remediation_plan": d.remediation_plan,
        "remediation_owner": d.remediation_owner,
        "status": d.status,
        "identified_date": _iso(d.identified_date),
        "remediated_date": _iso(d.remediated_date),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _def_from_node(dict(records[0]["x"]))


async def list_deficiencies(session: AsyncSession, user_id: str) -> List[Deficiency]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DEFICIENCY]->(x:SOXDeficiency)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_def_from_node(dict(r["x"])) async for r in result]


async def get_deficiency(session: AsyncSession, user_id: str, deficiency_id: str) -> Optional[Deficiency]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DEFICIENCY]->(x:SOXDeficiency)
    WHERE x.id = $deficiency_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, deficiency_id=deficiency_id, user_id=user_id)
    record = await result.single()
    return _def_from_node(dict(record["x"])) if record else None


async def save_deficiency(session: AsyncSession, user_id: str, d: Deficiency) -> None:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_DEFICIENCY]->(x:SOXDeficiency)
    WHERE x.id = $id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.control_id = $control_id,
        x.severity = $severity,
        x.description = $description,
        x.remediation_plan = $remediation_plan,
        x.remediation_owner = $remediation_owner,
        x.status = $status,
        x.identified_date = datetime($identified_date),
        x.remediated_date = datetime($remediated_date)
    """
    params = {
        "id": d.id,
        "control_id": d.control_id,
        "severity": d.severity,
        "description": d.description,
        "remediation_plan": d.remediation_plan,
        "remediation_owner": d.remediation_owner,
        "status": d.status,
        "identified_date": _iso(d.identified_date),
        "remediated_date": _iso(d.remediated_date),
    }
    await _run(session, query, params, user_id=user_id)


async def list_all_tests(session: AsyncSession, user_id: str) -> List[ControlTest]:
    """All of the caller's Book-visible control tests."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_TEST]->(x:SOXControlTest)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_test_from_node(dict(r["x"])) async for r in result]
