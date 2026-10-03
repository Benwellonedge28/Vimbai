"""
Capital Reconstruction Service CRUD Operations

Reconstructions, adjustments, and reserve conversions persist as Neo4j
nodes, caller-owned (X-User-Id) and Book-gated (X-Book-ID). Adjustments
and conversions are also linked to their parent reconstruction, but
authorization is enforced in the endpoint layer (caller must own the
parent) before appending; child writes are stamped with the caller's
user_id/book_id so they only surface in that caller's listings.
"""

import json
from datetime import datetime, timezone
from typing import Dict, List, Optional

from capital_reconstruction_service.dependencies import book_id_var
from capital_reconstruction_service.models import CapitalReconstruction, ReconstructionAdjustment, ReserveConversion
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


# --- reconstructions ---


def _recon_from_node(n: Dict) -> CapitalReconstruction:
    def _f(key: str, default: float = 0.0) -> float:
        v = n.get(key)
        return float(v) if v is not None else default

    return CapitalReconstruction(
        id=n["id"],
        company_id=n.get("company_id", ""),
        reconstruction_type=n.get("reconstruction_type", ""),
        description=n.get("description", ""),
        scheme_date=_as_dt(n.get("scheme_date")) or datetime.now(timezone.utc),
        court_approval_date=_as_dt(n.get("court_approval_date")),
        shareholders_approval_date=_as_dt(n.get("shareholders_approval_date")),
        previous_share_capital=_f("previous_share_capital"),
        new_share_capital=_f("new_share_capital"),
        capital_reduction_amount=_f("capital_reduction_amount"),
        share_consolidation_ratio=n.get("share_consolidation_ratio", ""),
        journal_entry_id=n.get("journal_entry_id"),
        status=n.get("status", "draft"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


def _recon_params(r: CapitalReconstruction) -> Dict:
    return {
        "id": r.id,
        "company_id": r.company_id,
        "reconstruction_type": r.reconstruction_type,
        "description": r.description,
        "scheme_date": _iso(r.scheme_date),
        "court_approval_date": _iso(r.court_approval_date),
        "shareholders_approval_date": _iso(r.shareholders_approval_date),
        "previous_share_capital": r.previous_share_capital,
        "new_share_capital": r.new_share_capital,
        "capital_reduction_amount": r.capital_reduction_amount,
        "share_consolidation_ratio": r.share_consolidation_ratio,
        "journal_entry_id": r.journal_entry_id,
        "status": r.status,
        "created_at": _iso(r.created_at),
    }


_RECON_PROPS = """id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        reconstruction_type: $reconstruction_type,
        description: $description,
        scheme_date: datetime($scheme_date),
        court_approval_date: datetime($court_approval_date),
        shareholders_approval_date: datetime($shareholders_approval_date),
        previous_share_capital: toFloat($previous_share_capital),
        new_share_capital: toFloat($new_share_capital),
        capital_reduction_amount: toFloat($capital_reduction_amount),
        share_consolidation_ratio: $share_consolidation_ratio,
        journal_entry_id: $journal_entry_id,
        status: $status,
        created_at: datetime($created_at)"""

_RECON_SET = """x.id = $id,
        x.company_id = $company_id,
        x.reconstruction_type = $reconstruction_type,
        x.description = $description,
        x.scheme_date = datetime($scheme_date),
        x.court_approval_date = datetime($court_approval_date),
        x.shareholders_approval_date = datetime($shareholders_approval_date),
        x.previous_share_capital = toFloat($previous_share_capital),
        x.new_share_capital = toFloat($new_share_capital),
        x.capital_reduction_amount = toFloat($capital_reduction_amount),
        x.share_consolidation_ratio = $share_consolidation_ratio,
        x.journal_entry_id = $journal_entry_id,
        x.status = $status,
        x.created_at = datetime($created_at)"""


async def create_reconstruction(session: AsyncSession, user_id: str, r: CapitalReconstruction) -> CapitalReconstruction:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:CapitalReconstruction {{
        {_RECON_PROPS}
    }})
    CREATE (u)-[:OWNS_RECONSTRUCTION]->(x)
    RETURN x
    """
    result = await _run(session, query, _recon_params(r), user_id=user_id)
    records = [rec async for rec in result]
    return _recon_from_node(dict(records[0]["x"]))


async def list_reconstructions(session: AsyncSession, user_id: str) -> List[CapitalReconstruction]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RECONSTRUCTION]->(x:CapitalReconstruction)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_recon_from_node(dict(r["x"])) async for r in result]


async def get_reconstruction(
    session: AsyncSession, user_id: str, reconstruction_id: str
) -> Optional[CapitalReconstruction]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RECONSTRUCTION]->(x:CapitalReconstruction)
    WHERE x.id = $reconstruction_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, reconstruction_id=reconstruction_id, user_id=user_id)
    record = await result.single()
    return _recon_from_node(dict(record["x"])) if record else None


async def save_reconstruction(session: AsyncSession, user_id: str, r: CapitalReconstruction) -> None:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_RECONSTRUCTION]->(x:CapitalReconstruction)
    WHERE x.id = $id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET {_RECON_SET}
    """
    await _run(session, query, _recon_params(r), user_id=user_id)


# --- adjustments ---


def _adj_from_node(n: Dict) -> ReconstructionAdjustment:
    def _f(key: str, default: float = 0.0) -> float:
        v = n.get(key)
        return float(v) if v is not None else default

    return ReconstructionAdjustment(
        id=n["id"],
        reconstruction_id=n.get("reconstruction_id", ""),
        account_code=n.get("account_code", ""),
        account_name=n.get("account_name", ""),
        previous_balance=_f("previous_balance"),
        adjustment_type=n.get("adjustment_type", ""),
        adjustment_amount=_f("adjustment_amount"),
        new_balance=_f("new_balance"),
        description=n.get("description", ""),
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_adjustment(
    session: AsyncSession, user_id: str, a: ReconstructionAdjustment
) -> ReconstructionAdjustment:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ReconstructionAdjustment {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        reconstruction_id: $reconstruction_id,
        account_code: $account_code,
        account_name: $account_name,
        previous_balance: toFloat($previous_balance),
        adjustment_type: $adjustment_type,
        adjustment_amount: toFloat($adjustment_amount),
        new_balance: toFloat($new_balance),
        description: $description,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_ADJUSTMENT]->(x)
    RETURN x
    """
    params = {
        "id": a.id,
        "reconstruction_id": a.reconstruction_id,
        "account_code": a.account_code,
        "account_name": a.account_name,
        "previous_balance": a.previous_balance,
        "adjustment_type": a.adjustment_type,
        "adjustment_amount": a.adjustment_amount,
        "new_balance": a.new_balance,
        "description": a.description,
        "journal_entry_id": a.journal_entry_id,
        "created_at": _iso(a.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _adj_from_node(dict(records[0]["x"]))


async def list_adjustments(
    session: AsyncSession, user_id: str, reconstruction_id: str
) -> List[ReconstructionAdjustment]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ADJUSTMENT]->(x:ReconstructionAdjustment)
    WHERE x.reconstruction_id = $reconstruction_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, reconstruction_id=reconstruction_id, user_id=user_id)
    return [_adj_from_node(dict(r["x"])) async for r in result]


# --- reserve conversions ---


def _conv_from_node(n: Dict) -> ReserveConversion:
    def _f(key: str, default: float = 0.0) -> float:
        v = n.get(key)
        return float(v) if v is not None else default

    return ReserveConversion(
        id=n["id"],
        reconstruction_id=n.get("reconstruction_id", ""),
        from_account=n.get("from_account", ""),
        from_account_name=n.get("from_account_name", ""),
        to_account=n.get("to_account", ""),
        to_account_name=n.get("to_account_name", ""),
        amount=_f("amount"),
        conversion_date=_as_dt(n.get("conversion_date")) or datetime.now(timezone.utc),
        reason=n.get("reason", ""),
        journal_entry_id=n.get("journal_entry_id"),
        created_at=_as_dt(n.get("created_at")) or datetime.now(timezone.utc),
    )


async def create_conversion(session: AsyncSession, user_id: str, c: ReserveConversion) -> ReserveConversion:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:ReserveConversion {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        reconstruction_id: $reconstruction_id,
        from_account: $from_account,
        from_account_name: $from_account_name,
        to_account: $to_account,
        to_account_name: $to_account_name,
        amount: toFloat($amount),
        conversion_date: datetime($conversion_date),
        reason: $reason,
        journal_entry_id: $journal_entry_id,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_CONVERSION]->(x)
    RETURN x
    """
    params = {
        "id": c.id,
        "reconstruction_id": c.reconstruction_id,
        "from_account": c.from_account,
        "from_account_name": c.from_account_name,
        "to_account": c.to_account,
        "to_account_name": c.to_account_name,
        "amount": c.amount,
        "conversion_date": _iso(c.conversion_date),
        "reason": c.reason,
        "journal_entry_id": c.journal_entry_id,
        "created_at": _iso(c.created_at),
    }
    result = await _run(session, query, params, user_id=user_id)
    records = [rec async for rec in result]
    return _conv_from_node(dict(records[0]["x"]))


async def list_conversions(session: AsyncSession, user_id: str, reconstruction_id: str) -> List[ReserveConversion]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CONVERSION]->(x:ReserveConversion)
    WHERE x.reconstruction_id = $reconstruction_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, reconstruction_id=reconstruction_id, user_id=user_id)
    return [_conv_from_node(dict(r["x"])) async for r in result]
