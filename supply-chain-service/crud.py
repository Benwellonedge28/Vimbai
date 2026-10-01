"""
Supply Chain Service CRUD Operations

Suppliers, inventory items and purchase orders move from in-memory
dicts (_suppliers / _inventory / _orders) to Neo4j: :Supplier nodes
via :OWNS_SUPPLIER, :InventoryItem nodes via :OWNS_ITEM and
:PurchaseOrder nodes via :OWNS_PO, book_id stamped, Book-gated.
Demand forecasting stays a pure computation (engine semantics in
main.py); its reorder check reads the caller's Book-visible
inventory. Receiving a PO updates the matching inventory item's
quantity (computed in Python, written back to the node).
"""

import json
import uuid
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from supply_chain_service.dependencies import book_id_var
from supply_chain_service.exceptions import NotFoundError
from supply_chain_service.models import InventoryItem, PurchaseOrder, Supplier

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


# --------------------------------------------------------------------------
# Suppliers
# --------------------------------------------------------------------------


def _supplier_from_node(n: Dict[str, Any]) -> Supplier:
    return Supplier(
        id=n["id"],
        name=n["name"],
        contact=n.get("contact", ""),
        lead_time_days=int(n.get("lead_time_days", 7)),
        rating=float(n.get("rating", 5.0)),
        products=json.loads(n.get("products", "[]") or "[]"),
    )


async def create_supplier(session: AsyncSession, user_id: str, supplier: Supplier) -> Supplier:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:Supplier {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        name: $name,
        contact: $contact,
        lead_time_days: toInteger($lead_time_days),
        rating: toFloat($rating),
        products: $products
    }})
    CREATE (u)-[:OWNS_SUPPLIER]->(x)
    """
    await _run(
        session,
        query,
        id=supplier.id,
        user_id=user_id,
        name=supplier.name,
        contact=supplier.contact,
        lead_time_days=supplier.lead_time_days,
        rating=supplier.rating,
        products=json.dumps(supplier.products),
    )
    return supplier


async def list_suppliers(session: AsyncSession, user_id: str) -> List[Supplier]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SUPPLIER]->(x:Supplier)
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id)
    return [_supplier_from_node(dict(r["x"])) async for r in result]


# --------------------------------------------------------------------------
# Inventory
# --------------------------------------------------------------------------


def _item_from_node(n: Dict[str, Any]) -> InventoryItem:
    return InventoryItem(
        id=n["id"],
        sku=n["sku"],
        name=n["name"],
        company_id=n["company_id"],
        quantity=int(n.get("quantity", 0)),
        reorder_point=int(n.get("reorder_point", 10)),
        reorder_qty=int(n.get("reorder_qty", 50)),
        unit_cost=float(n.get("unit_cost", 0)),
        unit_price=float(n.get("unit_price", 0)),
        supplier_id=n.get("supplier_id"),
        lead_time_days=int(n.get("lead_time_days", 7)),
    )


async def add_inventory(session: AsyncSession, user_id: str, item: InventoryItem) -> InventoryItem:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:InventoryItem {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        sku: $sku,
        name: $name,
        company_id: $company_id,
        quantity: toInteger($quantity),
        reorder_point: toInteger($reorder_point),
        reorder_qty: toInteger($reorder_qty),
        unit_cost: toFloat($unit_cost),
        unit_price: toFloat($unit_price),
        supplier_id: $supplier_id,
        lead_time_days: toInteger($lead_time_days)
    }})
    CREATE (u)-[:OWNS_ITEM]->(x)
    """
    await _run(
        session,
        query,
        id=item.id,
        user_id=user_id,
        sku=item.sku,
        name=item.name,
        company_id=item.company_id,
        quantity=item.quantity,
        reorder_point=item.reorder_point,
        reorder_qty=item.reorder_qty,
        unit_cost=item.unit_cost,
        unit_price=item.unit_price,
        supplier_id=item.supplier_id,
        lead_time_days=item.lead_time_days,
    )
    return item


async def list_inventory(session: AsyncSession, user_id: str, company_id: str) -> List[InventoryItem]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_ITEM]->(x:InventoryItem {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    return [_item_from_node(dict(r["x"])) async for r in result]


async def find_item_by_sku(session: AsyncSession, user_id: str, company_id: str, sku: str) -> Optional[InventoryItem]:
    items = await list_inventory(session, user_id, company_id)
    return next((i for i in items if i.sku == sku), None)


async def update_item_quantity(session: AsyncSession, item_id: str, quantity: int) -> None:
    query = "MATCH (x:InventoryItem {id: $id}) SET x.quantity = toInteger($quantity)"
    await _run(session, query, id=item_id, quantity=quantity)


# --------------------------------------------------------------------------
# Purchase Orders
# --------------------------------------------------------------------------


def _po_from_node(n: Dict[str, Any]) -> PurchaseOrder:
    return PurchaseOrder(
        id=n["id"],
        company_id=n["company_id"],
        supplier_id=n["supplier_id"],
        item_sku=n["item_sku"],
        quantity=int(n.get("quantity", 0)),
        unit_cost=float(n.get("unit_cost", 0)),
        status=n.get("status", "pending"),
        order_date=n.get("order_date"),
        expected_delivery=n.get("expected_delivery"),
    )


async def create_po(session: AsyncSession, user_id: str, po: PurchaseOrder) -> PurchaseOrder:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:PurchaseOrder {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        supplier_id: $supplier_id,
        item_sku: $item_sku,
        quantity: toInteger($quantity),
        unit_cost: toFloat($unit_cost),
        status: $status,
        order_date: $order_date,
        expected_delivery: $expected_delivery
    }})
    CREATE (u)-[:OWNS_PO]->(x)
    """
    await _run(
        session,
        query,
        id=po.id,
        user_id=user_id,
        company_id=po.company_id,
        supplier_id=po.supplier_id,
        item_sku=po.item_sku,
        quantity=po.quantity,
        unit_cost=po.unit_cost,
        status=po.status,
        order_date=po.order_date,
        expected_delivery=po.expected_delivery,
    )
    return po


async def list_pos(session: AsyncSession, user_id: str, company_id: str, status: str = "") -> List[PurchaseOrder]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_PO]->(x:PurchaseOrder {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    orders = [_po_from_node(dict(r["x"])) async for r in result]
    if status:
        orders = [o for o in orders if o.status == status]
    return orders


async def receive_po(session: AsyncSession, user_id: str, company_id: str, po_id: str) -> Dict[str, Any]:
    """Mark a PO received and add its quantity to the matching inventory item."""
    orders = await list_pos(session, user_id, company_id)
    po = next((o for o in orders if o.id == po_id), None)
    if po is None:
        raise NotFoundError("PO not found")
    query = "MATCH (x:PurchaseOrder {id: $id}) SET x.status = $status"
    await _run(session, query, id=po_id, status="received")
    item = await find_item_by_sku(session, user_id, company_id, po.item_sku)
    if item:
        await update_item_quantity(session, item.id, item.quantity + po.quantity)
    # original semantics: quantity_added reflects the PO quantity regardless
    # of whether a matching inventory item exists
    return {"po_id": po_id, "status": "received", "quantity_added": po.quantity}
