import os
import uuid
from datetime import datetime, timezone
from decimal import Decimal
from typing import Any, Dict, List, Optional

import httpx
from neo4j import AsyncSession
from supply_chain_service.exceptions import ConflictError, NotFoundError, ValidationError
from supply_chain_service.models import InventoryItemInDB  # NEW
from supply_chain_service.models import PurchaseOrderItemBase  # NEW
from supply_chain_service.models import SalesInvoiceItemBase  # Renamed Invoice to SalesInvoice
from supply_chain_service.models import SupplierInDB  # NEW
from supply_chain_service.models import (
    CustomerCreate,
    CustomerInDB,
    CustomerUpdate,
    InventoryItemCreate,
    InventoryItemUpdate,
    PurchaseOrderCreate,
    PurchaseOrderInDB,
    PurchaseOrderUpdate,
    SalesInvoiceCreate,
    SalesInvoiceInDB,
    SalesInvoiceUpdate,
    SupplierCreate,
    SupplierUpdate,
)

API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")


# --- Customer CRUD (unchanged from original invoicing service) ---
async def create_customer(session: AsyncSession, user_id: str, customer_data: CustomerCreate) -> CustomerInDB:
    customer_neo4j_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    # Check for existing customer with same name/email for this user
    existing_customer_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CUSTOMER]->(c:Customer)
    WHERE c.name = $name OR c.email = $email
    RETURN c
    """
    existing_customer_result = await session.run(
        existing_customer_query, user_id=user_id, name=customer_data.name, email=customer_data.email
    )
    if await existing_customer_result.single():
        raise ConflictError(
            detail="Customer with this name or email already exists for this user.", code="CUSTOMER_EXISTS"
        )

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (c:Customer {
        id: $id,
        name: $name,
        email: $email,
        phone: $phone,
        address: $address,
        tax_id: $tax_id,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_CUSTOMER]->(c)
    RETURN c
    """
    params = customer_data.model_dump()
    params["id"] = customer_neo4j_id
    params["user_id"] = user_id
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    customer_node = record["c"]

    return CustomerInDB(
        id=customer_node["id"],
        user_id=user_id,
        name=customer_node["name"],
        email=customer_node["email"],
        phone=customer_node["phone"],
        address=customer_node["address"],
        tax_id=customer_node["tax_id"],
        created_at=datetime.fromisoformat(customer_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(customer_node["updated_at"].iso_format()),
    )


async def get_customer(session: AsyncSession, user_id: str, customer_id: str) -> Optional[CustomerInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CUSTOMER]->(c:Customer {id: $customer_id})
    RETURN c
    """
    result = await session.run(query, user_id=user_id, customer_id=customer_id)
    record = await result.single()

    if record:
        customer_node = record["c"]
        return CustomerInDB(
            id=customer_node["id"],
            user_id=user_id,
            name=customer_node["name"],
            email=customer_node["email"],
            phone=customer_node["phone"],
            address=customer_node["address"],
            tax_id=customer_node["tax_id"],
            created_at=datetime.fromisoformat(customer_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(customer_node["updated_at"].iso_format()),
        )
    return None


async def get_all_customers(session: AsyncSession, user_id: str) -> List[CustomerInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CUSTOMER]->(c:Customer)
    RETURN c
    ORDER BY c.name
    """
    result = await session.run(query, user_id=user_id)
    customers = []
    async for record in result:
        customer_node = record["c"]
        customers.append(
            CustomerInDB(
                id=customer_node["id"],
                user_id=user_id,
                name=customer_node["name"],
                email=customer_node["email"],
                phone=customer_node["phone"],
                address=customer_node["address"],
                tax_id=customer_node["tax_id"],
                created_at=datetime.fromisoformat(customer_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(customer_node["updated_at"].iso_format()),
            )
        )
    return customers


async def update_customer(
    session: AsyncSession, user_id: str, customer_id: str, customer_data: CustomerUpdate
) -> Optional[CustomerInDB]:
    update_fields = customer_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_customer(session, user_id, customer_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()

    set_clauses = [f"c.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CUSTOMER]->(c:Customer {{id: $customer_id}})
    SET {set_query_part}
    RETURN c
    """

    params = {"user_id": user_id, "customer_id": customer_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_customer(session, user_id, customer_id)
    return None


async def delete_customer(session: AsyncSession, user_id: str, customer_id: str) -> bool:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CUSTOMER]->(c:Customer {id: $customer_id})
    DETACH DELETE c
    """
    result = await session.run(query, user_id=user_id, customer_id=customer_id)
    return result.consume().counters.nodes_deleted > 0


# --- Sales Invoice CRUD (renamed from invoice_crud) ---
async def create_sales_invoice(
    session: AsyncSession, user_id: str, invoice_data: SalesInvoiceCreate
) -> SalesInvoiceInDB:
    invoice_neo4j_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    # Verify customer exists and belongs to user
    customer = await get_customer(session, user_id, invoice_data.customer_id)
    if not customer:
        raise NotFoundError(detail="Customer not found or does not belong to user.", code="CUSTOMER_NOT_FOUND")

    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CUSTOMER]->(c:Customer {id: $customer_id})
    CREATE (i:SalesInvoice {
        id: $id,
        invoice_date: datetime($invoice_date),
        due_date: datetime($due_date),
        total_amount: toFloat($total_amount),
        currency: $currency,
        status: $status,
        notes: $notes,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (c)-[:ISSUED_INVOICE]->(i)
    RETURN i
    """
    params = invoice_data.model_dump(exclude={"items"})
    params["id"] = invoice_neo4j_id
    params["user_id"] = user_id
    params["invoice_date"] = params["invoice_date"].isoformat()
    params["due_date"] = params["due_date"].isoformat()
    params["total_amount"] = float(params["total_amount"])
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    invoice_node = record["i"]

    # Create SalesInvoiceItem nodes and link to SalesInvoice
    items_in_db = []
    for item_data in invoice_data.items:
        item_neo4j_id = str(uuid.uuid4())
        item_query = """
        MATCH (i:SalesInvoice {id: $invoice_id})
        CREATE (si:SalesInvoiceItem {
            id: $id,
            description: $description,
            quantity: $quantity,
            unit_price: toFloat($unit_price),
            line_total: toFloat($line_total)
        })
        CREATE (i)-[:HAS_ITEM]->(si)
        RETURN si
        """
        item_params = item_data.model_dump()
        item_params["id"] = item_neo4j_id
        item_params["invoice_id"] = invoice_neo4j_id
        item_params["unit_price"] = float(item_params["unit_price"])
        item_params["line_total"] = float(item_params["line_total"])

        item_result = await session.run(item_query, item_params)
        item_node = (await item_result.single())["si"]
        items_in_db.append(
            SalesInvoiceItemBase(
                description=item_node["description"],
                quantity=item_node["quantity"],
                unit_price=Decimal(str(item_node["unit_price"])),
                line_total=Decimal(str(item_node["line_total"])),
            )
        )

    return SalesInvoiceInDB(
        id=invoice_node["id"],
        user_id=user_id,
        customer_id=invoice_node["customer_id"],
        invoice_date=datetime.fromisoformat(invoice_node["invoice_date"].iso_format()),
        due_date=datetime.fromisoformat(invoice_node["due_date"].iso_format()),
        total_amount=Decimal(str(invoice_node["total_amount"])),
        currency=invoice_node["currency"],
        status=invoice_node["status"],
        notes=invoice_node["notes"],
        created_at=datetime.fromisoformat(invoice_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(invoice_node["updated_at"].iso_format()),
        items=items_in_db,
    )


async def get_sales_invoice(session: AsyncSession, user_id: str, invoice_id: str) -> Optional[SalesInvoiceInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CUSTOMER]->(c:Customer)-[:ISSUED_INVOICE]->(i:SalesInvoice {id: $invoice_id})
    OPTIONAL MATCH (i)-[:HAS_ITEM]->(si:SalesInvoiceItem)
    RETURN i, c.id AS customer_id, COLLECT(si) AS items_data
    """
    result = await session.run(query, user_id=user_id, invoice_id=invoice_id)
    record = await result.single()

    if record:
        invoice_node = record["i"]
        items_data = record["items_data"]

        items_in_db = []
        for item_node in items_data:
            if item_node:  # COLLECT can return [None] if no items
                items_in_db.append(
                    SalesInvoiceItemBase(
                        description=item_node["description"],
                        quantity=item_node["quantity"],
                        unit_price=Decimal(str(item_node["unit_price"])),
                        line_total=Decimal(str(item_node["line_total"])),
                    )
                )

        return SalesInvoiceInDB(
            id=invoice_node["id"],
            user_id=user_id,
            customer_id=record["customer_id"],
            invoice_date=datetime.fromisoformat(invoice_node["invoice_date"].iso_format()),
            due_date=datetime.fromisoformat(invoice_node["due_date"].iso_format()),
            total_amount=Decimal(str(invoice_node["total_amount"])),
            currency=invoice_node["currency"],
            status=invoice_node["status"],
            notes=invoice_node["notes"],
            created_at=datetime.fromisoformat(invoice_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(invoice_node["updated_at"].iso_format()),
            items=items_in_db,
        )
    return None


async def get_all_sales_invoices(session: AsyncSession, user_id: str) -> List[SalesInvoiceInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CUSTOMER]->(c:Customer)-[:ISSUED_INVOICE]->(i:SalesInvoice)
    OPTIONAL MATCH (i)-[:HAS_ITEM]->(si:SalesInvoiceItem)
    RETURN i, c.id AS customer_id, COLLECT(si) AS items_data
    ORDER BY i.invoice_date DESC
    """
    result = await session.run(query, user_id=user_id)

    invoices_map: Dict[str, SalesInvoiceInDB] = {}

    async for record in result:
        invoice_node = record["i"]
        items_data = record["items_data"]
        invoice_id = invoice_node["id"]

        if invoice_id not in invoices_map:
            invoices_map[invoice_id] = SalesInvoiceInDB(
                id=invoice_node["id"],
                user_id=user_id,
                customer_id=record["customer_id"],
                invoice_date=datetime.fromisoformat(invoice_node["invoice_date"].iso_format()),
                due_date=datetime.fromisoformat(invoice_node["due_date"].iso_format()),
                total_amount=Decimal(str(invoice_node["total_amount"])),
                currency=invoice_node["currency"],
                status=invoice_node["status"],
                notes=invoice_node["notes"],
                created_at=datetime.fromisoformat(invoice_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(invoice_node["updated_at"].iso_format()),
                items=[],
            )

        for item_node in items_data:
            if item_node:
                invoices_map[invoice_id].items.append(
                    SalesInvoiceItemBase(
                        description=item_node["description"],
                        quantity=item_node["quantity"],
                        unit_price=Decimal(str(item_node["unit_price"])),
                        line_total=Decimal(str(item_node["line_total"])),
                    )
                )

    return list(invoices_map.values())


async def update_sales_invoice(
    session: AsyncSession, user_id: str, invoice_id: str, invoice_data: SalesInvoiceUpdate
) -> Optional[SalesInvoiceInDB]:
    update_fields = invoice_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_sales_invoice(session, user_id, invoice_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    if "invoice_date" in update_fields and update_fields["invoice_date"]:
        update_fields["invoice_date"] = update_fields["invoice_date"].isoformat()
    if "due_date" in update_fields and update_fields["due_date"]:
        update_fields["due_date"] = update_fields["due_date"].isoformat()
    if "total_amount" in update_fields:
        update_fields["total_amount"] = float(update_fields["total_amount"])

    set_clauses = [f"i.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_CUSTOMER]->(c:Customer)-[:ISSUED_INVOICE]->(i:SalesInvoice {{id: $invoice_id}})
    SET {set_query_part}
    RETURN i
    """

    params = {"user_id": user_id, "invoice_id": invoice_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_sales_invoice(session, user_id, invoice_id)
    return None


async def delete_sales_invoice(session: AsyncSession, user_id: str, invoice_id: str) -> bool:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_CUSTOMER]->(c:Customer)-[:ISSUED_INVOICE]->(i:SalesInvoice {id: $invoice_id})
    OPTIONAL MATCH (i)-[:HAS_ITEM]->(si:SalesInvoiceItem)
    DETACH DELETE i, si
    """
    result = await session.run(query, user_id=user_id, invoice_id=invoice_id)
    return result.consume().counters.nodes_deleted > 0


# --- Supplier CRUD (NEW) ---
async def create_supplier(session: AsyncSession, user_id: str, supplier_data: SupplierCreate) -> SupplierInDB:
    supplier_neo4j_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    # Check for existing supplier with same name/email for this user
    existing_supplier_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SUPPLIER]->(s:Supplier)
    WHERE s.name = $name OR s.email = $email
    RETURN s
    """
    existing_supplier_result = await session.run(
        existing_supplier_query, user_id=user_id, name=supplier_data.name, email=supplier_data.email
    )
    if await existing_supplier_result.single():
        raise ConflictError(
            detail="Supplier with this name or email already exists for this user.", code="SUPPLIER_EXISTS"
        )

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (s:Supplier {
        id: $id,
        name: $name,
        contact_person: $contact_person,
        email: $email,
        phone: $phone,
        address: $address,
        tax_id: $tax_id,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_SUPPLIER]->(s)
    RETURN s
    """
    params = supplier_data.model_dump()
    params["id"] = supplier_neo4j_id
    params["user_id"] = user_id
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    supplier_node = record["s"]

    return SupplierInDB(
        id=supplier_node["id"],
        user_id=user_id,
        name=supplier_node["name"],
        contact_person=supplier_node["contact_person"],
        email=supplier_node["email"],
        phone=supplier_node["phone"],
        address=supplier_node["address"],
        tax_id=supplier_node["tax_id"],
        created_at=datetime.fromisoformat(supplier_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(supplier_node["updated_at"].iso_format()),
    )


async def get_supplier(session: AsyncSession, user_id: str, supplier_id: str) -> Optional[SupplierInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SUPPLIER]->(s:Supplier {id: $supplier_id})
    RETURN s
    """
    result = await session.run(query, user_id=user_id, supplier_id=supplier_id)
    record = await result.single()

    if record:
        supplier_node = record["s"]
        return SupplierInDB(
            id=supplier_node["id"],
            user_id=user_id,
            name=supplier_node["name"],
            contact_person=supplier_node["contact_person"],
            email=supplier_node["email"],
            phone=supplier_node["phone"],
            address=supplier_node["address"],
            tax_id=supplier_node["tax_id"],
            created_at=datetime.fromisoformat(supplier_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(supplier_node["updated_at"].iso_format()),
        )
    return None


async def get_all_suppliers(session: AsyncSession, user_id: str) -> List[SupplierInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SUPPLIER]->(s:Supplier)
    RETURN s
    ORDER BY s.name
    """
    result = await session.run(query, user_id=user_id)
    suppliers = []
    async for record in result:
        supplier_node = record["s"]
        suppliers.append(
            SupplierInDB(
                id=supplier_node["id"],
                user_id=user_id,
                name=supplier_node["name"],
                contact_person=supplier_node["contact_person"],
                email=supplier_node["email"],
                phone=supplier_node["phone"],
                address=supplier_node["address"],
                tax_id=supplier_node["tax_id"],
                created_at=datetime.fromisoformat(supplier_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(supplier_node["updated_at"].iso_format()),
            )
        )
    return suppliers


async def update_supplier(
    session: AsyncSession, user_id: str, supplier_id: str, supplier_data: SupplierUpdate
) -> Optional[SupplierInDB]:
    update_fields = supplier_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_supplier(session, user_id, supplier_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()

    set_clauses = [f"s.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SUPPLIER]->(s:Supplier {{id: $supplier_id}})
    SET {set_query_part}
    RETURN s
    """

    params = {"user_id": user_id, "supplier_id": supplier_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_supplier(session, user_id, supplier_id)
    return None


async def delete_supplier(session: AsyncSession, user_id: str, supplier_id: str) -> bool:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SUPPLIER]->(s:Supplier {id: $supplier_id})
    DETACH DELETE s
    """
    result = await session.run(query, user_id=user_id, supplier_id=supplier_id)
    return result.consume().counters.nodes_deleted > 0


# --- Inventory Item CRUD (NEW) ---
async def create_inventory_item(
    session: AsyncSession, user_id: str, item_data: InventoryItemCreate
) -> InventoryItemInDB:
    item_neo4j_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    # Verify preferred_supplier_id if provided
    if item_data.preferred_supplier_id:
        supplier = await get_supplier(session, user_id, item_data.preferred_supplier_id)
        if not supplier:
            raise NotFoundError(
                detail="Preferred supplier not found or does not belong to user.", code="SUPPLIER_NOT_FOUND"
            )

    # Check for existing item with same SKU for this user
    existing_item_query = """
    MATCH (u:User {id: $user_id})-[:OWNS_INVENTORY_ITEM]->(ii:InventoryItem)
    WHERE ii.sku = $sku
    RETURN ii
    """
    existing_item_result = await session.run(existing_item_query, user_id=user_id, sku=item_data.sku)
    if await existing_item_result.single():
        raise ConflictError(
            detail="Inventory item with this SKU already exists for this user.", code="INVENTORY_ITEM_EXISTS"
        )

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (ii:InventoryItem {
        id: $id,
        name: $name,
        sku: $sku,
        description: $description,
        unit_cost: toFloat($unit_cost),
        unit_of_measure: $unit_of_measure,
        current_stock: $current_stock,
        reorder_point: $reorder_point,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (u)-[:OWNS_INVENTORY_ITEM]->(ii)
    """
    params = item_data.model_dump(exclude={"preferred_supplier_id"})
    params["id"] = item_neo4j_id
    params["user_id"] = user_id
    params["unit_cost"] = float(params["unit_cost"])
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    await session.run(query, params)

    if item_data.preferred_supplier_id:
        link_query = """
        MATCH (ii:InventoryItem {id: $item_id})
        MATCH (s:Supplier {id: $supplier_id})
        CREATE (ii)-[:HAS_PREFERRED_SUPPLIER]->(s)
        """
        await session.run(link_query, item_id=item_neo4j_id, supplier_id=item_data.preferred_supplier_id)

    return await get_inventory_item(session, user_id, item_neo4j_id)  # Retrieve the full object with supplier_id


async def get_inventory_item(session: AsyncSession, user_id: str, item_id: str) -> Optional[InventoryItemInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_INVENTORY_ITEM]->(ii:InventoryItem {id: $item_id})
    OPTIONAL MATCH (ii)-[:HAS_PREFERRED_SUPPLIER]->(s:Supplier)
    RETURN ii, s.id AS preferred_supplier_id
    """
    result = await session.run(query, user_id=user_id, item_id=item_id)
    record = await result.single()

    if record:
        item_node = record["ii"]
        return InventoryItemInDB(
            id=item_node["id"],
            user_id=user_id,
            name=item_node["name"],
            sku=item_node["sku"],
            description=item_node["description"],
            unit_cost=Decimal(str(item_node["unit_cost"])),
            unit_of_measure=item_node["unit_of_measure"],
            current_stock=item_node["current_stock"],
            reorder_point=item_node["reorder_point"],
            preferred_supplier_id=record["preferred_supplier_id"],
            created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
        )
    return None


async def get_all_inventory_items(session: AsyncSession, user_id: str) -> List[InventoryItemInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_INVENTORY_ITEM]->(ii:InventoryItem)
    OPTIONAL MATCH (ii)-[:HAS_PREFERRED_SUPPLIER]->(s:Supplier)
    RETURN ii, s.id AS preferred_supplier_id
    ORDER BY ii.name
    """
    result = await session.run(query, user_id=user_id)
    items = []
    async for record in result:
        item_node = record["ii"]
        items.append(
            InventoryItemInDB(
                id=item_node["id"],
                user_id=user_id,
                name=item_node["name"],
                sku=item_node["sku"],
                description=item_node["description"],
                unit_cost=Decimal(str(item_node["unit_cost"])),
                unit_of_measure=item_node["unit_of_measure"],
                current_stock=item_node["current_stock"],
                reorder_point=item_node["reorder_point"],
                preferred_supplier_id=record["preferred_supplier_id"],
                created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
            )
        )
    return items


async def update_inventory_item(
    session: AsyncSession, user_id: str, item_id: str, item_data: InventoryItemUpdate
) -> Optional[InventoryItemInDB]:
    update_fields = item_data.model_dump(exclude_unset=True, exclude={"preferred_supplier_id"})
    if (
        not update_fields and "preferred_supplier_id" not in item_data.model_fields_set
    ):  # Check if supplier_id was specifically set to None
        return await get_inventory_item(session, user_id, item_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    if "unit_cost" in update_fields:
        update_fields["unit_cost"] = float(update_fields["unit_cost"])

    set_clauses = [f"ii.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_INVENTORY_ITEM]->(ii:InventoryItem {{id: $item_id}})
    SET {set_query_part}
    RETURN ii
    """

    params = {"user_id": user_id, "item_id": item_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if not record:
        return None

    # Handle preferred_supplier_id update
    if "preferred_supplier_id" in item_data.model_fields_set:  # If preferred_supplier_id was in the update payload
        # First, remove any existing HAS_PREFERRED_SUPPLIER relationship
        remove_link_query = """
        MATCH (ii:InventoryItem {id: $item_id})-[r:HAS_PREFERRED_SUPPLIER]->(s:Supplier)
        DELETE r
        """
        await session.run(remove_link_query, item_id=item_id)

        if item_data.preferred_supplier_id:  # If a new supplier ID is provided
            supplier = await get_supplier(session, user_id, item_data.preferred_supplier_id)
            if not supplier:
                raise NotFoundError(
                    detail="Preferred supplier not found or does not belong to user.", code="SUPPLIER_NOT_FOUND"
                )
            link_query = """
            MATCH (ii:InventoryItem {id: $item_id})
            MATCH (s:Supplier {id: $supplier_id})
            CREATE (ii)-[:HAS_PREFERRED_SUPPLIER]->(s)
            """
            await session.run(link_query, item_id=item_id, supplier_id=item_data.preferred_supplier_id)

    return await get_inventory_item(session, user_id, item_id)


async def delete_inventory_item(session: AsyncSession, user_id: str, item_id: str) -> bool:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_INVENTORY_ITEM]->(ii:InventoryItem {id: $item_id})
    DETACH DELETE ii
    """
    result = await session.run(query, user_id=user_id, item_id=item_id)
    return result.consume().counters.nodes_deleted > 0


# --- Purchase Order CRUD (NEW) ---
async def create_purchase_order(session: AsyncSession, user_id: str, po_data: PurchaseOrderCreate) -> PurchaseOrderInDB:
    po_neo4j_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    # Verify supplier exists and belongs to user
    supplier = await get_supplier(session, user_id, po_data.supplier_id)
    if not supplier:
        raise NotFoundError(detail="Supplier not found or does not belong to user.", code="SUPPLIER_NOT_FOUND")

    # Verify all inventory items exist and belong to user
    for po_item in po_data.items:
        inventory_item = await get_inventory_item(session, user_id, po_item.inventory_item_id)
        if not inventory_item:
            raise NotFoundError(
                detail=f"Inventory item {po_item.inventory_item_id} not found or does not belong to user.",
                code="INVENTORY_ITEM_NOT_FOUND",
            )

    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SUPPLIER]->(s:Supplier {id: $supplier_id})
    CREATE (po:PurchaseOrder {
        id: $id,
        order_date: datetime($order_date),
        expected_delivery_date: datetime($expected_delivery_date),
        total_amount: toFloat($total_amount),
        currency: $currency,
        status: $status,
        notes: $notes,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (s)-[:ISSUED_PO]->(po)
    RETURN po
    """
    params = po_data.model_dump(exclude={"items"})
    params["id"] = po_neo4j_id
    params["user_id"] = user_id
    params["order_date"] = params["order_date"].isoformat()
    if params["expected_delivery_date"]:
        params["expected_delivery_date"] = params["expected_delivery_date"].isoformat()
    params["total_amount"] = float(params["total_amount"])
    params["created_at"] = created_at.isoformat()
    params["updated_at"] = updated_at.isoformat()

    result = await session.run(query, params)
    record = await result.single()
    po_node = record["po"]

    # Create PurchaseOrderItem nodes and link to PurchaseOrder and InventoryItem
    items_in_db = []
    for item_data in po_data.items:
        po_item_neo4j_id = str(uuid.uuid4())
        po_item_query = """
        MATCH (po:PurchaseOrder {id: $po_id})
        MATCH (ii:InventoryItem {id: $inventory_item_id})
        CREATE (poi:PurchaseOrderItem {
            id: $id,
            quantity: $quantity,
            unit_price: toFloat($unit_price),
            line_total: toFloat($line_total)
        })
        CREATE (po)-[:HAS_ITEM]->(poi)
        CREATE (poi)-[:FOR_INVENTORY_ITEM]->(ii)
        RETURN poi, ii.id AS inventory_item_id
        """
        item_params = item_data.model_dump()
        item_params["id"] = po_item_neo4j_id
        item_params["po_id"] = po_neo4j_id
        item_params["unit_price"] = float(item_params["unit_price"])
        item_params["line_total"] = float(item_params["line_total"])

        item_result = await session.run(po_item_query, item_params)
        item_node = (await item_result.single())["poi"]
        items_in_db.append(
            PurchaseOrderItemBase(
                inventory_item_id=item_node["inventory_item_id"],
                quantity=item_node["quantity"],
                unit_price=Decimal(str(item_node["unit_price"])),
                line_total=Decimal(str(item_node["line_total"])),
            )
        )

    return PurchaseOrderInDB(
        id=po_node["id"],
        user_id=user_id,
        supplier_id=po_node["supplier_id"],
        order_date=datetime.fromisoformat(po_node["order_date"].iso_format()),
        expected_delivery_date=(
            datetime.fromisoformat(po_node["expected_delivery_date"].iso_format())
            if po_node["expected_delivery_date"]
            else None
        ),
        total_amount=Decimal(str(po_node["total_amount"])),
        currency=po_node["currency"],
        status=po_node["status"],
        notes=po_node["notes"],
        created_at=datetime.fromisoformat(po_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(po_node["updated_at"].iso_format()),
        items=items_in_db,
    )


async def get_purchase_order(session: AsyncSession, user_id: str, po_id: str) -> Optional[PurchaseOrderInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SUPPLIER]->(s:Supplier)-[:ISSUED_PO]->(po:PurchaseOrder {id: $po_id})
    OPTIONAL MATCH (po)-[:HAS_ITEM]->(poi:PurchaseOrderItem)-[:FOR_INVENTORY_ITEM]->(ii:InventoryItem)
    RETURN po, s.id AS supplier_id, COLLECT({
        inventory_item_id: ii.id,
        quantity: poi.quantity,
        unit_price: poi.unit_price,
        line_total: poi.line_total
    }) AS items_data
    """
    result = await session.run(query, user_id=user_id, po_id=po_id)
    record = await result.single()

    if record:
        po_node = record["po"]
        items_data = record["items_data"]

        items_in_db = []
        for item_data in items_data:
            if item_data and item_data.get("inventory_item_id"):
                items_in_db.append(
                    PurchaseOrderItemBase(
                        inventory_item_id=item_data["inventory_item_id"],
                        quantity=item_data["quantity"],
                        unit_price=Decimal(str(item_data["unit_price"])),
                        line_total=Decimal(str(item_data["line_total"])),
                    )
                )

        return PurchaseOrderInDB(
            id=po_node["id"],
            user_id=user_id,
            supplier_id=record["supplier_id"],
            order_date=datetime.fromisoformat(po_node["order_date"].iso_format()),
            expected_delivery_date=(
                datetime.fromisoformat(po_node["expected_delivery_date"].iso_format())
                if po_node["expected_delivery_date"]
                else None
            ),
            total_amount=Decimal(str(po_node["total_amount"])),
            currency=po_node["currency"],
            status=po_node["status"],
            notes=po_node["notes"],
            created_at=datetime.fromisoformat(po_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(po_node["updated_at"].iso_format()),
            items=items_in_db,
        )
    return None


async def get_all_purchase_orders(session: AsyncSession, user_id: str) -> List[PurchaseOrderInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SUPPLIER]->(s:Supplier)-[:ISSUED_PO]->(po:PurchaseOrder)
    OPTIONAL MATCH (po)-[:HAS_ITEM]->(poi:PurchaseOrderItem)-[:FOR_INVENTORY_ITEM]->(ii:InventoryItem)
    RETURN po, s.id AS supplier_id, COLLECT({
        inventory_item_id: ii.id,
        quantity: poi.quantity,
        unit_price: poi.unit_price,
        line_total: poi.line_total
    }) AS items_data
    ORDER BY po.order_date DESC
    """
    result = await session.run(query, user_id=user_id)

    pos_map: Dict[str, PurchaseOrderInDB] = {}

    async for record in result:
        po_node = record["po"]
        items_data = record["items_data"]
        po_id = po_node["id"]

        if po_id not in pos_map:
            pos_map[po_id] = PurchaseOrderInDB(
                id=po_node["id"],
                user_id=user_id,
                supplier_id=record["supplier_id"],
                order_date=datetime.fromisoformat(po_node["order_date"].iso_format()),
                expected_delivery_date=(
                    datetime.fromisoformat(po_node["expected_delivery_date"].iso_format())
                    if po_node["expected_delivery_date"]
                    else None
                ),
                total_amount=Decimal(str(po_node["total_amount"])),
                currency=po_node["currency"],
                status=po_node["status"],
                notes=po_node["notes"],
                created_at=datetime.fromisoformat(po_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(po_node["updated_at"].iso_format()),
                items=[],
            )

        for item_data in items_data:
            if item_data and item_data.get("inventory_item_id"):
                pos_map[po_id].items.append(
                    PurchaseOrderItemBase(
                        inventory_item_id=item_data["inventory_item_id"],
                        quantity=item_data["quantity"],
                        unit_price=Decimal(str(item_data["unit_price"])),
                        line_total=Decimal(str(item_data["line_total"])),
                    )
                )

    return list(pos_map.values())


async def update_purchase_order(
    session: AsyncSession, user_id: str, po_id: str, po_data: PurchaseOrderUpdate
) -> Optional[PurchaseOrderInDB]:
    update_fields = po_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_purchase_order(session, user_id, po_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    if "order_date" in update_fields and update_fields["order_date"]:
        update_fields["order_date"] = update_fields["order_date"].isoformat()
    if "expected_delivery_date" in update_fields and update_fields["expected_delivery_date"]:
        update_fields["expected_delivery_date"] = update_fields["expected_delivery_date"].isoformat()
    if "total_amount" in update_fields:
        update_fields["total_amount"] = float(update_fields["total_amount"])

    set_clauses = [f"po.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_SUPPLIER]->(s:Supplier)-[:ISSUED_PO]->(po:PurchaseOrder {{id: $po_id}})
    SET {set_query_part}
    RETURN po
    """

    params = {"user_id": user_id, "po_id": po_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_purchase_order(session, user_id, po_id)
    return None


async def delete_purchase_order(session: AsyncSession, user_id: str, po_id: str) -> bool:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_SUPPLIER]->(s:Supplier)-[:ISSUED_PO]->(po:PurchaseOrder {id: $po_id})
    OPTIONAL MATCH (po)-[:HAS_ITEM]->(poi:PurchaseOrderItem)
    DETACH DELETE po, poi
    """
    result = await session.run(query, user_id=user_id, po_id=po_id)
    return result.consume().counters.nodes_deleted > 0
