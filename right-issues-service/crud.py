"""
Right Issues Service CRUD Operations

Right issues and renunciations are nodes, Book-gated, so one caller's rights offer,
entitlement register and acceptances are invisible to another.

QUIRKS preserved:
* a missing right issue is not an HTTP error: the endpoints answer 200 with an
  {"error": ...} body, and that contract is kept;
* the completion test credits a shareholder one unit per renunciation, not the
  shares renounced, exactly as the original computed it;
* total proceeds are the new shares on offer times the issue price, worked out once
  at offer time;
* the renunciation journal entry debits and credits the same bank account, so it
  nets to nothing.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional, Tuple

from neo4j import AsyncSession
from right_issues_service.dependencies import book_id_var
from right_issues_service.exceptions import NotFoundError
from right_issues_service.models import Renunciation, RightIssue

RIGHT_ISSUE_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _as_dt(value) -> Optional[datetime]:
    if value is None:
        return None
    if isinstance(value, datetime):
        dt = value
    else:
        iso = value.iso_format() if hasattr(value, "iso_format") else str(value)
        if iso in (None, "None"):
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _json_map(raw, default):
    if not raw:
        return dict(default)
    try:
        return json.loads(raw)
    except (TypeError, ValueError):
        return dict(default)


def _issue_from_node(n: Dict, user_id: str) -> RightIssue:
    return RightIssue(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n.get("company_id") or "",
        issue_date=_as_dt(n.get("issue_date")) or _now(),
        expiry_date=_as_dt(n.get("expiry_date")) or _now(),
        share_class=n.get("share_class") or "",
        rights_offered=int(n.get("rights_offered") or 0),
        new_shares_offered=int(n.get("new_shares_offered") or 0),
        issue_price=float(n.get("issue_price") or 0),
        nominal_value=float(n.get("nominal_value") or 0),
        total_proceeds=float(n.get("total_proceeds") or 0),
        renouncement_allowed=bool(n.get("renouncement_allowed")),
        entitlements=_json_map(n.get("entitlements_json"), {}),
        acceptances=_json_map(n.get("acceptances_json"), {}),
        renunciations=_json_map(n.get("renunciations_json"), {}),
        journal_entry_id=n.get("journal_entry_id"),
        status=n.get("status") or "offered",
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _renunciation_from_node(n: Dict, user_id: str) -> Renunciation:
    return Renunciation(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        right_issue_id=n.get("right_issue_id") or "",
        original_shareholder_id=n.get("original_shareholder_id") or "",
        new_shareholder_id=n.get("new_shareholder_id") or "",
        renunciation_date=_as_dt(n.get("renunciation_date")) or _now(),
        renunciation_price=float(n.get("renunciation_price") or 0),
    )


async def offer_rights(
    session: AsyncSession,
    user_id: str,
    *,
    company_id: str,
    issue_date: datetime,
    expiry_date: datetime,
    share_class: str,
    rights_offered: int,
    new_shares_offered: int,
    issue_price: float,
    nominal_value: float,
    entitlements: Dict[str, int],
    renouncement_allowed: bool = True,
) -> RightIssue:
    issue = RightIssue(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        company_id=company_id,
        issue_date=issue_date,
        expiry_date=expiry_date,
        share_class=share_class,
        rights_offered=rights_offered,
        new_shares_offered=new_shares_offered,
        issue_price=issue_price,
        nominal_value=nominal_value,
        renouncement_allowed=renouncement_allowed,
        entitlements=dict(entitlements or {}),
    )
    issue.total_proceeds = new_shares_offered * issue_price

    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:RightIssue {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            company_id: $company_id,
            issue_date: datetime($issue_date),
            expiry_date: datetime($expiry_date),
            share_class: $share_class,
            rights_offered: $rights_offered,
            new_shares_offered: $new_shares_offered,
            issue_price: toFloat($issue_price),
            nominal_value: toFloat($nominal_value),
            total_proceeds: toFloat($total_proceeds),
            renouncement_allowed: $renouncement_allowed,
            entitlements_json: $entitlements_json,
            acceptances_json: $acceptances_json,
            renunciations_json: $renunciations_json,
            status: $status,
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_RIGHT_ISSUE]->(x)
        RETURN x
        """,
        {
            "id": issue.id,
            "user_id": user_id,
            "company_id": company_id,
            "issue_date": issue.issue_date.isoformat(),
            "expiry_date": issue.expiry_date.isoformat(),
            "share_class": share_class,
            "rights_offered": rights_offered,
            "new_shares_offered": new_shares_offered,
            "issue_price": issue_price,
            "nominal_value": nominal_value,
            "total_proceeds": issue.total_proceeds,
            "renouncement_allowed": renouncement_allowed,
            "entitlements_json": json.dumps(issue.entitlements),
            "acceptances_json": json.dumps({}),
            "renunciations_json": json.dumps({}),
            "status": issue.status,
            "created_at": issue.created_at.isoformat(),
        },
    )
    return issue


async def get_right_issue(session: AsyncSession, user_id: str, right_issue_id: str) -> RightIssue:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RIGHT_ISSUE]->(x:RightIssue {{id: $right_issue_id}})
    {RIGHT_ISSUE_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, right_issue_id=right_issue_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Right issue not found")
    return _issue_from_node(dict(records[0]["x"]), user_id)


async def list_right_issues(session: AsyncSession, user_id: str, company_id: Optional[str] = None) -> List[RightIssue]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RIGHT_ISSUE]->(x:RightIssue)
    {RIGHT_ISSUE_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    issues = [_issue_from_node(dict(r["x"]), user_id) async for r in result]
    if company_id:
        issues = [i for i in issues if i.company_id == company_id]
    return issues


async def record_acceptance(
    session: AsyncSession, user_id: str, right_issue_id: str, shareholder_id: str, shares_accepted: int
) -> RightIssue:
    issue = await get_right_issue(session, user_id, right_issue_id)

    issue.acceptances[shareholder_id] = shares_accepted
    issue.status = _status_after_acceptance(issue)

    await _run(
        session,
        """
        MATCH (x:RightIssue {id: $right_issue_id})
        SET x.acceptances_json = $acceptances_json, x.status = $status
        """,
        {
            "right_issue_id": right_issue_id,
            "acceptances_json": json.dumps(issue.acceptances),
            "status": issue.status,
        },
    )
    return issue


def _status_after_acceptance(issue: RightIssue) -> str:
    """Completed once every entitlement is covered by acceptances and renunciations.

    The original counted one unit per renunciation rather than the shares renounced;
    that is kept.
    """
    for shareholder in issue.entitlements:
        renounced = sum(1 for orig, _new in issue.renunciations.items() if orig == shareholder)
        if issue.entitlements.get(shareholder, 0) != issue.acceptances.get(shareholder, 0) + renounced:
            return issue.status
    return "completed"


async def record_renunciation(
    session: AsyncSession,
    user_id: str,
    right_issue_id: str,
    shareholder_id: str,
    new_shareholder_id: str,
    renunciation_price: float,
    renunciation_date: datetime,
) -> Tuple[RightIssue, Renunciation]:
    issue = await get_right_issue(session, user_id, right_issue_id)

    renunciation = Renunciation(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        right_issue_id=right_issue_id,
        original_shareholder_id=shareholder_id,
        new_shareholder_id=new_shareholder_id,
        renunciation_date=renunciation_date,
        renunciation_price=renunciation_price,
    )
    issue.renunciations[shareholder_id] = new_shareholder_id

    await _run(
        session,
        """
        MATCH (x:RightIssue {id: $right_issue_id})
        SET x.renunciations_json = $renunciations_json
        """,
        {"right_issue_id": right_issue_id, "renunciations_json": json.dumps(issue.renunciations)},
    )
    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:Renunciation {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            right_issue_id: $right_issue_id,
            original_shareholder_id: $original_shareholder_id,
            new_shareholder_id: $new_shareholder_id,
            renunciation_date: datetime($renunciation_date),
            renunciation_price: toFloat($renunciation_price)
        })
        CREATE (u)-[:OWNS_RENUNCIATION]->(x)
        RETURN x
        """,
        {
            "id": renunciation.id,
            "user_id": user_id,
            "right_issue_id": right_issue_id,
            "original_shareholder_id": shareholder_id,
            "new_shareholder_id": new_shareholder_id,
            "renunciation_date": renunciation_date.isoformat(),
            "renunciation_price": renunciation_price,
        },
    )
    return issue, renunciation


async def list_renunciations(session: AsyncSession, user_id: str, right_issue_id: str) -> List[Renunciation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RENUNCIATION]->(x:Renunciation {{right_issue_id: $right_issue_id}})
    {RIGHT_ISSUE_FILTER}
    RETURN x
    ORDER BY x.renunciation_date ASC
    """
    result = await _run(session, query, user_id=user_id, right_issue_id=right_issue_id)
    return [_renunciation_from_node(dict(r["x"]), user_id) async for r in result]
