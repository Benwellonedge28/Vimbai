"""
Fixed Assets Schedule Service CRUD Operations

:FixedAsset via :OWNS_FIXED_ASSET and :AssetSchedule via :OWNS_ASSET_SCHEDULE,
both Book-gated, so one caller's register is invisible to another.

QUIRKS preserved:
* registering an asset re-stamps the id and works out net book value as cost less
  accumulated depreciation, ignoring any net book value supplied;
* an update writes whatever fields it is given but never recomputes net book
  value, so lifting accumulated depreciation leaves the book value where it was;
* a category with nothing in it is left out of the summary rather than reported
  as zeroes;
* the latest schedule is generated on demand rather than read back.
"""

import json
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from fixed_assets_schedule_service.dependencies import book_id_var
from fixed_assets_schedule_service.exceptions import NotFoundError
from fixed_assets_schedule_service.models import (
    AssetCategory,
    AssetSchedule,
    AssetStatus,
    FixedAsset,
)
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

# asset fields an update is allowed to write; id and created_at are never touched
UPDATABLE = [
    "asset_code",
    "asset_name",
    "description",
    "category",
    "location",
    "custodian",
    "purchase_date",
    "purchase_cost",
    "residual_value",
    "useful_life_years",
    "depreciation_method",
    "accumulated_depreciation",
    "net_book_value",
    "status",
    "supplier",
    "warranty_expiry",
    "insurance_policy",
    "notes",
]

DATETIME_FIELDS = {"purchase_date", "warranty_expiry"}


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


def _from_node(n: Dict[str, Any], user_id: str) -> FixedAsset:
    return FixedAsset(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        asset_code=n.get("asset_code") or "",
        asset_name=n.get("asset_name") or "",
        description=n.get("description"),
        category=AssetCategory(n.get("category") or AssetCategory.LAND.value),
        location=n.get("location"),
        custodian=n.get("custodian"),
        purchase_date=_as_dt(n.get("purchase_date")) or _now(),
        purchase_cost=float(n.get("purchase_cost") or 0),
        residual_value=float(n.get("residual_value") or 0),
        useful_life_years=int(n.get("useful_life_years") or 0),
        depreciation_method=n.get("depreciation_method") or "straight_line",
        accumulated_depreciation=float(n.get("accumulated_depreciation") or 0),
        net_book_value=float(n.get("net_book_value") or 0),
        status=AssetStatus(n.get("status") or AssetStatus.IN_USE.value),
        supplier=n.get("supplier"),
        warranty_expiry=_as_dt(n.get("warranty_expiry")),
        insurance_policy=n.get("insurance_policy"),
        notes=n.get("notes"),
        created_at=_as_dt(n.get("created_at")) or _now(),
        updated_at=_as_dt(n.get("updated_at")) or _now(),
    )


async def register_asset(session: AsyncSession, user_id: str, data: FixedAsset) -> FixedAsset:
    data.id = str(uuid.uuid4())
    data.user_id = user_id
    data.book_id = book_id_var.get()
    data.net_book_value = data.purchase_cost - data.accumulated_depreciation
    data.created_at = _now()
    data.updated_at = _now()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:FixedAsset {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        asset_code: $asset_code,
        asset_name: $asset_name,
        description: $description,
        category: $category,
        location: $location,
        custodian: $custodian,
        purchase_date: datetime($purchase_date),
        purchase_cost: toFloat($purchase_cost),
        residual_value: toFloat($residual_value),
        useful_life_years: $useful_life_years,
        depreciation_method: $depreciation_method,
        accumulated_depreciation: toFloat($accumulated),
        net_book_value: toFloat($nbv),
        status: $status,
        supplier: $supplier,
        warranty_expiry: $warranty_expiry,
        insurance_policy: $insurance_policy,
        notes: $notes,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_FIXED_ASSET]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": data.id,
            "user_id": user_id,
            "asset_code": data.asset_code,
            "asset_name": data.asset_name,
            "description": data.description,
            "category": data.category.value,
            "location": data.location,
            "custodian": data.custodian,
            "purchase_date": _iso(data.purchase_date),
            "purchase_cost": data.purchase_cost,
            "residual_value": data.residual_value,
            "useful_life_years": data.useful_life_years,
            "depreciation_method": data.depreciation_method,
            "accumulated": data.accumulated_depreciation,
            "nbv": data.net_book_value,
            "status": data.status.value,
            "supplier": data.supplier,
            "warranty_expiry": _iso(data.warranty_expiry),
            "insurance_policy": data.insurance_policy,
            "notes": data.notes,
            "created_at": data.created_at.isoformat(),
            "updated_at": data.updated_at.isoformat(),
        },
    )
    return data


async def list_assets(
    session: AsyncSession,
    user_id: str,
    category: Optional[AssetCategory] = None,
    status: Optional[AssetStatus] = None,
) -> List[FixedAsset]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FIXED_ASSET]->(x:FixedAsset)
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.created_at ASC
    """
    result = await _run(session, query, user_id=user_id)
    assets = [_from_node(dict(r["x"]), user_id) async for r in result]
    if category:
        assets = [a for a in assets if a.category == category]
    if status:
        assets = [a for a in assets if a.status == status]
    return assets


async def get_asset(session: AsyncSession, user_id: str, asset_id: str) -> FixedAsset:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_FIXED_ASSET]->(x:FixedAsset {{id: $asset_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, asset_id=asset_id)
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Asset not found")
    return _from_node(dict(records[0]["x"]), user_id)


async def update_asset(session: AsyncSession, user_id: str, asset_id: str, changes: Dict[str, Any]) -> FixedAsset:
    asset = await get_asset(session, user_id, asset_id)

    updates: Dict[str, Any] = {}
    for key, value in changes.items():
        if key in UPDATABLE:
            updates[key] = value

    # net book value is only ever written when it is handed over: an update never
    # recomputes it, which is how the original behaved
    params = {"id": asset_id, "updated_at": _now().isoformat()}
    sets = ["x.updated_at = datetime($updated_at)"]
    for key, value in updates.items():
        if key in DATETIME_FIELDS:
            sets.append(f"x.{key} = datetime(${key})")
            params[key] = _iso(_as_dt(value))
        elif key in ("purchase_cost", "residual_value", "accumulated_depreciation", "net_book_value"):
            sets.append(f"x.{key} = toFloat(${key})")
            params[key] = float(value)
        elif key == "useful_life_years":
            sets.append(f"x.{key} = ${key}")
            params[key] = int(value)
        else:
            sets.append(f"x.{key} = ${key}")
            params[key] = value.value if hasattr(value, "value") else value

    await _run(session, f"MATCH (x:FixedAsset {{id: $id}}) SET {', '.join(sets)}", params)
    return await get_asset(session, user_id, asset_id)


def _summarise(assets: List[FixedAsset]) -> Dict[str, Dict[str, float]]:
    summary: Dict[str, Dict[str, float]] = {}
    for category in AssetCategory:
        bucket = [a for a in assets if a.category == category]
        if bucket:
            summary[category.value] = {
                "count": len(bucket),
                "cost": sum(a.purchase_cost for a in bucket),
                "accumulated": sum(a.accumulated_depreciation for a in bucket),
                "nbv": sum(a.net_book_value for a in bucket),
            }
    return summary


async def generate_schedule(session: AsyncSession, user_id: str, as_of_date: datetime) -> AssetSchedule:
    assets = await list_assets(session, user_id)
    schedule = AssetSchedule(
        id=str(uuid.uuid4()),
        user_id=user_id,
        book_id=book_id_var.get(),
        as_of_date=as_of_date,
        assets=assets,
        total_cost=sum(a.purchase_cost for a in assets),
        total_accumulated_depreciation=sum(a.accumulated_depreciation for a in assets),
        total_net_book_value=sum(a.net_book_value for a in assets),
        category_summary=_summarise(assets),
        created_at=_now(),
    )
    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:AssetSchedule {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        as_of_date: datetime($as_of_date),
        asset_count: $asset_count,
        total_cost: toFloat($total_cost),
        total_accumulated_depreciation: toFloat($total_accumulated),
        total_net_book_value: toFloat($total_nbv),
        category_summary_json: $category_summary,
        created_at: datetime($created_at)
    })
    CREATE (u)-[:OWNS_ASSET_SCHEDULE]->(x)
    RETURN x
    """
    await _run(
        session,
        query,
        {
            "id": schedule.id,
            "user_id": user_id,
            "as_of_date": _iso(as_of_date),
            "asset_count": len(assets),
            "total_cost": schedule.total_cost,
            "total_accumulated": schedule.total_accumulated_depreciation,
            "total_nbv": schedule.total_net_book_value,
            "category_summary": json.dumps(schedule.category_summary),
            "created_at": schedule.created_at.isoformat(),
        },
    )
    return schedule


async def summarise(session: AsyncSession, user_id: str) -> Dict[str, Any]:
    assets = await list_assets(session, user_id)
    return {
        "total_assets": len(assets),
        "total_cost": sum(a.purchase_cost for a in assets),
        "total_accumulated_depreciation": sum(a.accumulated_depreciation for a in assets),
        "total_net_book_value": sum(a.net_book_value for a in assets),
        "by_category": _summarise(assets),
    }
