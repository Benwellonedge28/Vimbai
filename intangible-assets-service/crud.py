"""
Intangible Assets Service CRUD Operations

:IntangibleAsset via :OWNS_INTANGIBLE_ASSET and :AmortizationEntry via
:OWNS_AMORTIZATION_ENTRY, both Book-gated, so one caller's intangible register and
its amortisation history are invisible to another.

QUIRKS preserved:
* an asset is registered at its full cost, whatever residual value it carries;
* the reducing balance method is the service's own simplification: ten per cent of
  the excess over residual, spread over twelve months;
* the book value is recomputed as cost less accumulated amortisation, so an asset
  that is already at residual value gets clamped back to it and marked
  fully_amortised, while the accumulated figure keeps climbing;
* amortising anything that is not active answers 400, not a conflict.
"""

import uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional

from intangible_assets_service.dependencies import book_id_var
from intangible_assets_service.exceptions import NotFoundError
from intangible_assets_service.models import AmortizationEntry, IntangibleAsset
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

VALID_TYPES = ["goodwill", "patent", "trademark", "copyright", "software", "license"]


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


def _asset_from_node(n: Dict, user_id: str) -> IntangibleAsset:
    return IntangibleAsset(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        name=n.get("name") or "",
        asset_type=n.get("asset_type") or "",
        cost=float(n.get("cost") or 0),
        useful_life_years=int(n.get("useful_life_years") or 0),
        residual_value=float(n.get("residual_value") or 0),
        acquisition_date=_as_dt(n.get("acquisition_date")) or _now(),
        amortization_method=n.get("amortization_method") or "straight_line",
        accumulated_amortization=float(n.get("accumulated_amortization") or 0),
        net_book_value=float(n.get("net_book_value") or 0),
        status=n.get("status") or "active",
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _entry_from_node(n: Dict, user_id: str) -> AmortizationEntry:
    return AmortizationEntry(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        asset_id=n.get("asset_id") or "",
        period=n.get("period") or "",
        amortization_amount=float(n.get("amortization_amount") or 0),
        accumulated_amortization=float(n.get("accumulated_amortization") or 0),
        net_book_value=float(n.get("net_book_value") or 0),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


async def create_asset(
    session: AsyncSession,
    user_id: str,
    *,
    name: str,
    asset_type: str,
    cost: float,
    useful_life_years: int,
    acquisition_date: datetime,
    residual_value: float = 0.0,
    amortization_method: str = "straight_line",
) -> IntangibleAsset:
    asset = IntangibleAsset(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        name=name,
        asset_type=asset_type,
        cost=cost,
        useful_life_years=useful_life_years,
        residual_value=residual_value,
        acquisition_date=acquisition_date,
        amortization_method=amortization_method,
        net_book_value=cost,
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:IntangibleAsset {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        asset_type: $asset_type,
        cost: toFloat($cost),
        useful_life_years: $useful_life_years,
        residual_value: toFloat($residual_value),
        acquisition_date: datetime($acquisition_date),
        amortization_method: $amortization_method,
        accumulated_amortization: 0.0,
        net_book_value: toFloat($cost),
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_INTANGIBLE_ASSET]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": asset.id,
            "user_id": user_id,
            "name": name,
            "asset_type": asset_type,
            "cost": cost,
            "useful_life_years": useful_life_years,
            "residual_value": residual_value,
            "acquisition_date": _iso(acquisition_date),
            "amortization_method": amortization_method,
            "status": asset.status,
            "created_at": asset.created_at.isoformat(),
        },
    )
    return asset


async def list_assets(
    session: AsyncSession,
    user_id: str,
    asset_type: Optional[str] = None,
    status: Optional[str] = None,
) -> List[IntangibleAsset]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_INTANGIBLE_ASSET]->(x:IntangibleAsset)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    assets = [_asset_from_node(dict(r["x"]), user_id) async for r in result]
    if asset_type:
        assets = [a for a in assets if a.asset_type == asset_type]
    if status:
        assets = [a for a in assets if a.status == status]
    return assets


async def get_asset(session: AsyncSession, user_id: str, asset_id: str) -> IntangibleAsset:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_INTANGIBLE_ASSET]->(x:IntangibleAsset {{id: $asset_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, asset_id=asset_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Asset not found")
    return _asset_from_node(dict(records[0]["x"]), user_id)


async def amortize_asset(session: AsyncSession, user_id: str, asset_id: str, period: str) -> AmortizationEntry:
    asset = await get_asset(session, user_id, asset_id)

    if asset.amortization_method == "straight_line":
        annual_amort = (asset.cost - asset.residual_value) / asset.useful_life_years
        monthly_amort = annual_amort / 12
    else:
        monthly_amort = (asset.net_book_value - asset.residual_value) * 0.1 / 12

    accumulated = asset.accumulated_amortization + monthly_amort
    net_book_value = asset.cost - accumulated
    status = asset.status
    if net_book_value <= asset.residual_value:
        net_book_value = asset.residual_value
        status = "fully_amortized"

    await _run(
        session,
        """
        MATCH (x:IntangibleAsset {id: $id})
        SET x.accumulated_amortization = toFloat($accumulated),
            x.net_book_value = toFloat($nbv),
            x.status = $status
        """,
        {"id": asset_id, "accumulated": accumulated, "nbv": net_book_value, "status": status},
    )

    entry = AmortizationEntry(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        asset_id=asset_id,
        period=period,
        amortization_amount=monthly_amort,
        accumulated_amortization=accumulated,
        net_book_value=net_book_value,
    )
    await _run(
        session,
        """
        MATCH (u:User {id: $user_id})
        CREATE (x:AmortizationEntry {
            id: $id,
            user_id: $user_id,
            book_id: $book_id,
            asset_id: $asset_id,
            period: $period,
            amortization_amount: toFloat($amount),
            accumulated_amortization: toFloat($accumulated),
            net_book_value: toFloat($nbv),
            created_at: datetime($created_at)
        })
        CREATE (u)-[:OWNS_AMORTIZATION_ENTRY]->(x)
        RETURN x
        """,
        {
            "id": entry.id,
            "user_id": user_id,
            "asset_id": asset_id,
            "period": period,
            "amount": monthly_amort,
            "accumulated": accumulated,
            "nbv": net_book_value,
            "created_at": entry.created_at.isoformat(),
        },
    )
    return entry


async def list_amortization(session: AsyncSession, user_id: str, asset_id: str) -> List[AmortizationEntry]:
    """An unknown or foreign asset simply has no entries, as before."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_AMORTIZATION_ENTRY]->(x:AmortizationEntry {{asset_id: $asset_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id, asset_id=asset_id)
    return [_entry_from_node(dict(r["x"]), user_id) async for r in result]
