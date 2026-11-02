"""
Retained Profits Service CRUD Operations

:ProfitAppropriation via :OWNS_APPROPRIATION, :InterimDividend via
:OWNS_INTERIM_DIVIDEND and :PriorYearAdjustment via :OWNS_PRIOR_ADJUSTMENT,
all Book-gated. The summary a company id asks for is read from the caller's own
records, so another company's retained earnings can no longer be inspected.

PYTHON QUIRKS preserved:
* an appropriation closes at opening retained earnings plus what it carried
  forward, and its total is simply the sum of the named allocations;
* an interim dividend takes the amount it is given, without checking it against
  per_share_amount times total_shares;
* paying an unknown dividend answers 200 with {"error": ...}, not a 404;
* a prior year adjustment books a credit to trade receivables whatever type it
  claims to be.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from retained_profits_service.dependencies import book_id_var
from retained_profits_service.exceptions import NotFoundError
from retained_profits_service.models import (
    InterimDividend,
    PriorYearAdjustment,
    ProfitAppropriation,
)

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


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


def _iso(value: Optional[datetime]) -> Optional[str]:
    return value.isoformat() if value else None


def _appropriation_from_node(n: Dict[str, Any], user_id: str) -> ProfitAppropriation:
    return ProfitAppropriation(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n.get("company_id") or "",
        financial_year=n.get("financial_year") or "",
        period_start=_as_dt(n.get("period_start")) or _now(),
        period_end=_as_dt(n.get("period_end")) or _now(),
        net_profit=float(n.get("net_profit") or 0),
        appropriations=json.loads(n.get("appropriations_json") or "{}"),
        total_appropriations=float(n.get("total_appropriations") or 0),
        balance_carried_forward=float(n.get("balance_carried_forward") or 0),
        opening_retained_earnings=float(n.get("opening_retained_earnings") or 0),
        closing_retained_earnings=float(n.get("closing_retained_earnings") or 0),
        journal_entry_id=n.get("journal_entry_id"),
        status=n.get("status") or "draft",
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _dividend_from_node(n: Dict[str, Any], user_id: str) -> InterimDividend:
    return InterimDividend(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n.get("company_id") or "",
        amount=float(n.get("amount") or 0),
        per_share_amount=float(n.get("per_share_amount") or 0),
        total_shares=int(n.get("total_shares") or 0),
        declaration_date=_as_dt(n.get("declaration_date")) or _now(),
        payment_date=_as_dt(n.get("payment_date")),
        journal_entry_id=n.get("journal_entry_id"),
        status=n.get("status") or "declared",
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _adjustment_from_node(n: Dict[str, Any], user_id: str) -> PriorYearAdjustment:
    return PriorYearAdjustment(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n.get("company_id") or "",
        adjustment_type=n.get("adjustment_type") or "",
        description=n.get("description") or "",
        amount=float(n.get("amount") or 0),
        affected_year=n.get("affected_year") or "",
        journal_entry_id=n.get("journal_entry_id"),
        adjustment_date=_as_dt(n.get("adjustment_date")) or _now(),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def create_appropriation(
    session: AsyncSession,
    user_id: str,
    *,
    company_id: str,
    financial_year: str,
    period_start: datetime,
    period_end: datetime,
    net_profit: float,
    opening_retained_earnings: float,
    appropriations: Dict[str, float],
    journal_entry_id: Optional[str] = None,
    status: str = "approved",
) -> ProfitAppropriation:
    record = ProfitAppropriation(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        company_id=company_id,
        financial_year=financial_year,
        period_start=period_start,
        period_end=period_end,
        net_profit=net_profit,
        appropriations=appropriations,
        opening_retained_earnings=opening_retained_earnings,
        journal_entry_id=journal_entry_id,
        status=status,
    )
    record.total_appropriations = sum(appropriations.values())
    record.balance_carried_forward = net_profit - record.total_appropriations
    record.closing_retained_earnings = opening_retained_earnings + record.balance_carried_forward

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ProfitAppropriation {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        financial_year: $financial_year,
        period_start: datetime($period_start),
        period_end: datetime($period_end),
        net_profit: toFloat($net_profit),
        appropriations_json: $appropriations_json,
        total_appropriations: toFloat($total),
        balance_carried_forward: toFloat($carried),
        opening_retained_earnings: toFloat($opening),
        closing_retained_earnings: toFloat($closing),
        journal_entry_id: $journal_entry_id,
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_APPROPRIATION]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": record.id,
            "user_id": user_id,
            "company_id": company_id,
            "financial_year": financial_year,
            "period_start": _iso(period_start),
            "period_end": _iso(period_end),
            "net_profit": net_profit,
            "appropriations_json": json.dumps(appropriations),
            "total": record.total_appropriations,
            "carried": record.balance_carried_forward,
            "opening": opening_retained_earnings,
            "closing": record.closing_retained_earnings,
            "journal_entry_id": journal_entry_id,
            "status": status,
            "created_at": _now().isoformat(),
        },
    )
    return record


async def list_appropriations(
    session: AsyncSession,
    user_id: str,
    company_id: Optional[str] = None,
    financial_year: Optional[str] = None,
) -> List[ProfitAppropriation]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_APPROPRIATION]->(x:ProfitAppropriation)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    records = [_appropriation_from_node(dict(r["x"]), user_id) async for r in result]
    if company_id:
        records = [a for a in records if a.company_id == company_id]
    if financial_year:
        records = [a for a in records if a.financial_year == financial_year]
    return records


async def declare_dividend(
    session: AsyncSession,
    user_id: str,
    *,
    company_id: str,
    amount: float,
    per_share_amount: float,
    total_shares: int,
    declaration_date: datetime,
    journal_entry_id: Optional[str] = None,
) -> InterimDividend:
    dividend = InterimDividend(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        company_id=company_id,
        amount=amount,
        per_share_amount=per_share_amount,
        total_shares=total_shares,
        declaration_date=declaration_date,
        journal_entry_id=journal_entry_id,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:InterimDividend {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        amount: toFloat($amount),
        per_share_amount: toFloat($per_share),
        total_shares: $total_shares,
        declaration_date: datetime($declaration_date),
        status: $status,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_INTERIM_DIVIDEND]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": dividend.id,
            "user_id": user_id,
            "company_id": company_id,
            "amount": amount,
            "per_share": per_share_amount,
            "total_shares": total_shares,
            "declaration_date": _iso(declaration_date),
            "status": dividend.status,
            "journal_entry_id": journal_entry_id,
            "created_at": _now().isoformat(),
        },
    )
    return dividend


async def list_dividends(
    session: AsyncSession, user_id: str, company_id: Optional[str] = None
) -> List[InterimDividend]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_INTERIM_DIVIDEND]->(x:InterimDividend)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    dividends = [_dividend_from_node(dict(r["x"]), user_id) async for r in result]
    if company_id:
        dividends = [d for d in dividends if d.company_id == company_id]
    return dividends


async def pay_dividend(
    session: AsyncSession, user_id: str, dividend_id: str, payment_date: datetime
) -> InterimDividend:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_INTERIM_DIVIDEND]->(x:InterimDividend {{id: $dividend_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, dividend_id=dividend_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Dividend not found")

    await _run(
        session,
        """
        MATCH (x:InterimDividend {id: $id})
        SET x.payment_date = datetime($payment_date), x.status = $status
        """,
        {"id": dividend_id, "payment_date": _iso(payment_date), "status": "paid"},
    )
    dividend = _dividend_from_node(dict(records[0]["x"]), user_id)
    dividend.payment_date = payment_date
    dividend.status = "paid"
    return dividend


async def create_adjustment(
    session: AsyncSession,
    user_id: str,
    *,
    company_id: str,
    adjustment_type: str,
    description: str,
    amount: float,
    affected_year: str,
    adjustment_date: Optional[datetime] = None,
    journal_entry_id: Optional[str] = None,
) -> PriorYearAdjustment:
    adjustment = PriorYearAdjustment(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        company_id=company_id,
        adjustment_type=adjustment_type,
        description=description,
        amount=amount,
        affected_year=affected_year,
        adjustment_date=adjustment_date or _now(),
        journal_entry_id=journal_entry_id,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:PriorYearAdjustment {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        adjustment_type: $adjustment_type,
        description: $description,
        amount: toFloat($amount),
        affected_year: $affected_year,
        adjustment_date: datetime($adjustment_date),
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_PRIOR_ADJUSTMENT]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": adjustment.id,
            "user_id": user_id,
            "company_id": company_id,
            "adjustment_type": adjustment_type,
            "description": description,
            "amount": amount,
            "affected_year": affected_year,
            "adjustment_date": _iso(adjustment.adjustment_date),
            "journal_entry_id": journal_entry_id,
            "created_at": _now().isoformat(),
        },
    )
    return adjustment


async def list_adjustments(
    session: AsyncSession, user_id: str, company_id: Optional[str] = None
) -> List[PriorYearAdjustment]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PRIOR_ADJUSTMENT]->(x:PriorYearAdjustment)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    adjustments = [_adjustment_from_node(dict(r["x"]), user_id) async for r in result]
    if company_id:
        adjustments = [a for a in adjustments if a.company_id == company_id]
    return adjustments


async def summarise(session: AsyncSession, user_id: str, company_id: str) -> Dict[str, Any]:
    company_appropriations = await list_appropriations(session, user_id, company_id=company_id)
    company_adjustments = await list_adjustments(session, user_id, company_id=company_id)

    total_carried_forward = sum(a.balance_carried_forward for a in company_appropriations)
    total_adjustments = sum(a.amount for a in company_adjustments)

    return {
        "company_id": company_id,
        "total_carried_forward": total_carried_forward,
        "total_prior_year_adjustments": total_adjustments,
        "net_retained_earnings": total_carried_forward + total_adjustments,
        "last_appropriation": company_appropriations[-1] if company_appropriations else None,
    }
