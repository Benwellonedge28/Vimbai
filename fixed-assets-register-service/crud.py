"""
Fixed Assets Register Service CRUD Operations

Assets, depreciation entries, and disposals persist as Neo4j nodes,
caller-owned (X-User-Id) and Book-gated (X-Book-ID). Depreciation
arithmetic stays pure Python and is written back to the asset node via
params (the fake test harness cannot do node-prop arithmetic in SET).
Cross-scope reads/updates raise NotFoundError -> 404, matching the
original "Asset not found" semantics.
"""

from datetime import datetime, timezone
from typing import Dict, List, Optional

from fixed_assets_register_service.dependencies import book_id_var
from fixed_assets_register_service.exceptions import NotFoundError
from fixed_assets_register_service.models import AssetDisposal, DepreciationEntry, FixedAsset
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session: AsyncSession, query: str, params: Optional[Dict] = None, **kw):
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
        if iso is None or iso == "None":
            return None
        dt = datetime.fromisoformat(iso)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt


def _asset_from_node(n: Dict) -> FixedAsset:
    return FixedAsset(
        id=n["id"],
        asset_code=n["asset_code"],
        asset_name=n["asset_name"],
        category=n["category"],
        location=n.get("location", ""),
        department=n.get("department", ""),
        acquisition_date=_as_dt(n.get("acquisition_date")) or _now(),
        acquisition_cost=float(n.get("acquisition_cost", 0.0)),
        useful_life_years=int(n.get("useful_life_years", 0)),
        salvage_value=float(n.get("salvage_value", 0.0)),
        depreciation_method=n.get("depreciation_method", "straight_line"),
        accumulated_depreciation=float(n.get("accumulated_depreciation", 0.0)),
        net_book_value=float(n.get("net_book_value", 0.0)),
        status=n.get("status", "active"),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _entry_from_node(n: Dict) -> DepreciationEntry:
    return DepreciationEntry(
        id=n["id"],
        asset_id=n["asset_id"],
        period=n["period"],
        depreciation_amount=float(n.get("depreciation_amount", 0.0)),
        accumulated_depreciation=float(n.get("accumulated_depreciation", 0.0)),
        net_book_value=float(n.get("net_book_value", 0.0)),
        method=n.get("method", "straight_line"),
        created_at=_as_dt(n.get("created_at")) or _now(),
    )


def _disposal_from_node(n: Dict) -> AssetDisposal:
    return AssetDisposal(
        id=n["id"],
        asset_id=n["asset_id"],
        disposal_date=_as_dt(n.get("disposal_date")) or _now(),
        disposal_value=float(n.get("disposal_value", 0.0)),
        disposal_method=n.get("disposal_method", "sale"),
        gain_loss=float(n.get("gain_loss", 0.0)),
        notes=n.get("notes", ""),
    )


async def _list(session: AsyncSession, user_id: str, label: str, edge: str) -> List[Dict]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [dict(r["x"]) async for r in result]


async def _get_by_id(session: AsyncSession, user_id: str, label: str, edge: str, node_id: str) -> Optional[Dict]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:{edge}]->(x:{label})
    WHERE x.id = $node_id AND ($book_id IS NULL OR x.book_id = $book_id)
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, node_id=node_id)
    record = await result.single()
    return dict(record["x"]) if record else None


# --- assets ---


async def create_asset(session: AsyncSession, user_id: str, asset: FixedAsset) -> FixedAsset:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:FixedAsset {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        asset_code: $asset_code,
        asset_name: $asset_name,
        category: $category,
        location: $location,
        department: $department,
        acquisition_date: datetime($acquisition_date),
        acquisition_cost: toFloat($acquisition_cost),
        useful_life_years: toInteger($useful_life_years),
        salvage_value: toFloat($salvage_value),
        depreciation_method: $depreciation_method,
        accumulated_depreciation: toFloat($accumulated_depreciation),
        net_book_value: toFloat($net_book_value),
        status: $status,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_ASSET]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": asset.id,
            "asset_code": asset.asset_code,
            "asset_name": asset.asset_name,
            "category": asset.category,
            "location": asset.location,
            "department": asset.department,
            "acquisition_date": asset.acquisition_date.isoformat(),
            "acquisition_cost": asset.acquisition_cost,
            "useful_life_years": asset.useful_life_years,
            "salvage_value": asset.salvage_value,
            "depreciation_method": asset.depreciation_method,
            "accumulated_depreciation": asset.accumulated_depreciation,
            "net_book_value": asset.net_book_value,
            "status": asset.status,
            "created_at": asset.created_at.isoformat(),
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _asset_from_node(dict(records[0]["x"]))


async def list_assets(session: AsyncSession, user_id: str) -> List[FixedAsset]:
    return [_asset_from_node(n) for n in await _list(session, user_id, "FixedAsset", "OWNS_ASSET")]


async def get_asset(session: AsyncSession, user_id: str, asset_id: str) -> Optional[FixedAsset]:
    """Return the caller's Book-visible asset, or None (caller maps to 404)."""
    node = await _get_by_id(session, user_id, "FixedAsset", "OWNS_ASSET", asset_id)
    return _asset_from_node(node) if node else None


async def update_asset_state(
    session: AsyncSession,
    user_id: str,
    asset_id: str,
    accumulated_depreciation: float,
    net_book_value: float,
    status: str,
) -> None:
    """Write back depreciation-driven state (Python-computed values)."""
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_ASSET]->(x:FixedAsset)
    WHERE x.id = $asset_id AND ($book_id IS NULL OR x.book_id = $book_id)
    SET x.accumulated_depreciation = toFloat($accumulated_depreciation),
        x.net_book_value = toFloat($net_book_value),
        x.status = $status
    """
    await _run(
        session,
        query,
        {
            "accumulated_depreciation": accumulated_depreciation,
            "net_book_value": net_book_value,
            "status": status,
        },
        user_id=user_id,
        asset_id=asset_id,
    )


# --- depreciation ---


async def create_entry(session: AsyncSession, user_id: str, entry: DepreciationEntry) -> DepreciationEntry:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:DepreciationEntry {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        asset_id: $asset_id,
        period: $period,
        depreciation_amount: toFloat($depreciation_amount),
        accumulated_depreciation: toFloat($accumulated_depreciation),
        net_book_value: toFloat($net_book_value),
        method: $method,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_DEPRECIATION]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": entry.id,
            "asset_id": entry.asset_id,
            "period": entry.period,
            "depreciation_amount": entry.depreciation_amount,
            "accumulated_depreciation": entry.accumulated_depreciation,
            "net_book_value": entry.net_book_value,
            "method": entry.method,
            "created_at": entry.created_at.isoformat(),
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _entry_from_node(dict(records[0]["x"]))


async def list_entries(session: AsyncSession, user_id: str, asset_id: str) -> List[DepreciationEntry]:
    entries = [_entry_from_node(n) for n in await _list(session, user_id, "DepreciationEntry", "OWNS_DEPRECIATION")]
    return [e for e in entries if e.asset_id == asset_id]


# --- disposals ---


async def create_disposal(session: AsyncSession, user_id: str, disposal: AssetDisposal) -> AssetDisposal:
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:AssetDisposal {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        asset_id: $asset_id,
        disposal_date: datetime($disposal_date),
        disposal_value: toFloat($disposal_value),
        disposal_method: $disposal_method,
        gain_loss: toFloat($gain_loss),
        notes: $notes
    })
    CREATE (u)-[:OWNS_DISPOSAL]->(x)
    RETURN x
    """
    result = await _run(
        session,
        query,
        {
            "id": disposal.id,
            "asset_id": disposal.asset_id,
            "disposal_date": disposal.disposal_date.isoformat(),
            "disposal_value": disposal.disposal_value,
            "disposal_method": disposal.disposal_method,
            "gain_loss": disposal.gain_loss,
            "notes": disposal.notes,
        },
        user_id=user_id,
    )
    records = [r async for r in result]
    return _disposal_from_node(dict(records[0]["x"]))
