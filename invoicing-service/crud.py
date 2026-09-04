import os
import uuid
from datetime import datetime, timezone
from decimal import Decimal
from typing import Any, Dict, List, Optional

import httpx
from invoicing_service.exceptions import ValidationError  # NEW
from invoicing_service.models import (
    CreateJournalEntryResponse,
    CustomerCreate,
    CustomerInDB,
    CustomerUpdate,
    InvoiceCreate,
    InvoiceInDB,
    InvoiceItemCreate,
    InvoiceItemInDB,
    InvoiceUpdate,
    JournalEntryCreate,
    JournalLineBase,
)
from invoicing_service.dependencies import book_id_var
from neo4j import AsyncSession

API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound.

    ``book_id`` comes from the request-scoped X-Book-ID header (verified by
    the gateway); it is None for personal/unscoped calls.
    """
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


# --- Customer CRUD ---
async def create_customer(session: AsyncSession, user_id: str, customer_data: CustomerCreate) -> CustomerInDB:
    customer_neo4j_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    props = customer_data.model_dump()
    props["id"] = customer_neo4j_id
    props["user_id"] = user_id
    props["book_id"] = book_id_var.get()
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = created_at.isoformat()

    query = """
    CREATE (c:Customer $props)
    RETURN c
    """
    result = await session.run(query, props=props)
    node = (await result.single())["c"]
    return _customer_from_node(node)


async def get_customer_by_id(session: AsyncSession, customer_id: str, user_id: str) -> Optional[CustomerInDB]:
    query = """
    MATCH (c:Customer {customer_id: $customer_id, user_id: $user_id})
    WHERE $book_id IS NULL OR c.book_id = $book_id
    RETURN c
    """
    result = await _run(session, query, customer_id=customer_id, user_id=user_id)
    record = await result.single()
    if record:
        return _customer_from_node(record["c"])
    return None


async def get_all_customers(session: AsyncSession, user_id: str) -> List[CustomerInDB]:
    query = """
    MATCH (c:Customer {user_id: $user_id})
    WHERE $book_id IS NULL OR c.book_id = $book_id
    RETURN c
    ORDER BY c.name
    """
    result = await _run(session, query, user_id=user_id)
    customers = []
    async for record in result:
        customers.append(_customer_from_node(record["c"]))
    return customers


async def update_customer(
    session: AsyncSession, customer_id: str, user_id: str, customer_data: CustomerUpdate
) -> Optional[CustomerInDB]:
    update_fields = customer_data.model_dump(exclude_unset=True)
    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    set_query_part = ", ".join(f"c.{k} = ${k}" for k in update_fields)

    query = f"""
    MATCH (c:Customer {{customer_id: $customer_id, user_id: $user_id}})
    WHERE $book_id IS NULL OR c.book_id = $book_id
    SET {set_query_part}
    RETURN c
    """
    result = await _run(session, query, customer_id=customer_id, user_id=user_id, **update_fields)
    record = await result.single()
    if record:
        return _customer_from_node(record["c"])
    return None


async def delete_customer(session: AsyncSession, customer_id: str, user_id: str) -> bool:
    query = """
    MATCH (c:Customer {customer_id: $customer_id, user_id: $user_id})
    WHERE $book_id IS NULL OR c.book_id = $book_id
    DETACH DELETE c
    """
    result = await _run(session, query, customer_id=customer_id, user_id=user_id)
    return result.consume().counters.nodes_deleted > 0


def _customer_from_node(node) -> CustomerInDB:
    props = dict(node)
    props.pop("book_id", None)  # Book scoping marker, not part of the API model
    return CustomerInDB(**props)


# --- Invoice CRUD ---
async def create_invoice(session: AsyncSession, user_id: str, invoice_data: InvoiceCreate) -> InvoiceInDB:
    invoice_neo4j_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    # Create Invoice node and link to Customer
    invoice_query = """
    MATCH (c:Customer {customer_id: $customer_id, user_id: $user_id})
    WHERE $book_id IS NULL OR c.book_id = $book_id
    CREATE (i:Invoice {
        id: $id,
        invoice_number: $invoice_number,
        book_id: $book_id,
        invoice_date: datetime($invoice_date),
        due_date: datetime($due_date),
        total_amount: toFloat($total_amount),
        status: $status,
        notes: $notes,
        created_at: datetime($created_at),
        updated_at: datetime($updated_at)
    })
    CREATE (c)-[:HAS_INVOICE]->(i)
    RETURN i
    """
    invoice_params = invoice_data.model_dump(exclude={"items"})
    invoice_params["id"] = invoice_neo4j_id
    invoice_params["user_id"] = user_id  # Passed for matching customer
    invoice_params["invoice_date"] = invoice_params["invoice_date"].isoformat()
    invoice_params["due_date"] = invoice_params["due_date"].isoformat()
    invoice_params["total_amount"] = float(invoice_params["total_amount"])
    invoice_params["created_at"] = created_at.isoformat()
    invoice_params["updated_at"] = updated_at.isoformat()

    result = await _run(session, invoice_query, invoice_params)
    record = await result.single()
    invoice_node = record["i"]

    # Create InvoiceItem nodes and link to Invoice
    invoice_items_in_db = []
    for item_data in invoice_data.items:
        item_id = str(uuid.uuid4())

        item_query = """
        MATCH (i:Invoice {id: $invoice_id})
        CREATE (ii:InvoiceItem {
            id: $item_id,
            description: $description,
            quantity: toFloat($quantity),
            unit_price: toFloat($unit_price),
            amount: toFloat($amount),
            account_number: $account_number,
            created_at: datetime($created_at),
            updated_at: datetime($updated_at)
        })
        CREATE (i)-[:HAS_ITEM]->(ii)
        RETURN ii
        """
        item_params = item_data.model_dump()
        item_params["id"] = item_id
        item_params["invoice_id"] = invoice_neo4j_id
        item_params["quantity"] = float(item_params["quantity"])
        item_params["unit_price"] = float(item_params["unit_price"])
        item_params["amount"] = float(item_params["amount"])
        # InvoiceItemCreate has no timestamp fields; stamp with the invoice's
        item_params["created_at"] = created_at.isoformat()
        item_params["updated_at"] = updated_at.isoformat()

        item_result = await session.run(item_query, item_params)
        item_node = (await item_result.single())["ii"]
        invoice_items_in_db.append(
            InvoiceItemInDB(
                id=item_node["id"],
                description=item_node["description"],
                quantity=Decimal(str(item_node["quantity"])),
                unit_price=Decimal(str(item_node["unit_price"])),
                amount=Decimal(str(item_node["amount"])),
                account_number=item_node["account_number"],
                created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
            )
        )

    return InvoiceInDB(
        id=invoice_node["id"],
        customer_id=invoice_data.customer_id,
        invoice_number=invoice_node["invoice_number"],
        invoice_date=datetime.fromisoformat(invoice_node["invoice_date"].iso_format()),
        due_date=datetime.fromisoformat(invoice_node["due_date"].iso_format()),
        total_amount=Decimal(str(invoice_node["total_amount"])),
        status=invoice_node["status"],
        notes=invoice_node["notes"],
        items=invoice_items_in_db,
        created_at=datetime.fromisoformat(invoice_node["created_at"].iso_format()),
        updated_at=datetime.fromisoformat(invoice_node["updated_at"].iso_format()),
    )


async def get_invoice_by_number(session: AsyncSession, invoice_number: str, user_id: str) -> Optional[InvoiceInDB]:
    query = """
    MATCH (c:Customer {user_id: $user_id})-[:HAS_INVOICE]->(i:Invoice {invoice_number: $invoice_number})
    WHERE $book_id IS NULL OR i.book_id = $book_id
    OPTIONAL MATCH (i)-[:HAS_ITEM]->(ii:InvoiceItem)
    RETURN i, COLLECT(ii) AS items, c.customer_id AS customer_id
    """
    result = await _run(session, query, invoice_number=invoice_number, user_id=user_id)
    record = await result.single()

    if record:
        invoice_node = record["i"]
        items_data = record["items"]
        customer_id = record["customer_id"]  # Get customer_id directly from the RETURN clause

        invoice_items_in_db = []
        for item_node in items_data:
            if item_node:  # Ensure item_node is not None (COLLECT can return [None] if no items)
                invoice_items_in_db.append(
                    InvoiceItemInDB(
                        id=item_node["id"],
                        description=item_node["description"],
                        quantity=Decimal(str(item_node["quantity"])),
                        unit_price=Decimal(str(item_node["unit_price"])),
                        amount=Decimal(str(item_node["amount"])),
                        account_number=item_node["account_number"],
                        created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
                        updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
                    )
                )

        return InvoiceInDB(
            id=invoice_node["id"],
            customer_id=customer_id,
            invoice_number=invoice_node["invoice_number"],
            invoice_date=datetime.fromisoformat(invoice_node["invoice_date"].iso_format()),
            due_date=datetime.fromisoformat(invoice_node["due_date"].iso_format()),
            total_amount=Decimal(str(invoice_node["total_amount"])),
            status=invoice_node["status"],
            notes=invoice_node["notes"],
            items=invoice_items_in_db,
            created_at=datetime.fromisoformat(invoice_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(invoice_node["updated_at"].iso_format()),
        )
    return None


async def get_all_invoices(session: AsyncSession, user_id: str) -> List[InvoiceInDB]:
    query = """
    MATCH (c:Customer {user_id: $user_id})-[:HAS_INVOICE]->(i:Invoice)
    WHERE $book_id IS NULL OR i.book_id = $book_id
    OPTIONAL MATCH (i)-[:HAS_ITEM]->(ii:InvoiceItem)
    RETURN i, COLLECT(ii) AS items, c.customer_id AS customer_id
    ORDER BY i.invoice_date DESC
    """
    result = await _run(session, query, user_id=user_id)

    invoices = []
    # Group items by invoice
    invoice_map: Dict[str, InvoiceInDB] = {}

    async for record in result:
        invoice_node = record["i"]
        items_data = record["items"]
        customer_id = record["customer_id"]
        invoice_id = invoice_node["id"]

        if invoice_id not in invoice_map:
            invoice_map[invoice_id] = InvoiceInDB(
                id=invoice_node["id"],
                customer_id=customer_id,
                invoice_number=invoice_node["invoice_number"],
                invoice_date=datetime.fromisoformat(invoice_node["invoice_date"].iso_format()),
                due_date=datetime.fromisoformat(invoice_node["due_date"].iso_format()),
                total_amount=Decimal(str(invoice_node["total_amount"])),
                status=invoice_node["status"],
                notes=invoice_node["notes"],
                items=[],  # Initialize empty list for items
                created_at=datetime.fromisoformat(invoice_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(invoice_node["updated_at"].iso_format()),
            )

        for item_node in items_data:
            if item_node:  # Only add if item_node is not None (for invoices with no items)
                invoice_map[invoice_id].items.append(
                    InvoiceItemInDB(
                        id=item_node["id"],
                        description=item_node["description"],
                        quantity=Decimal(str(item_node["quantity"])),
                        unit_price=Decimal(str(item_node["unit_price"])),
                        amount=Decimal(str(item_node["amount"])),
                        account_number=item_node["account_number"],
                        created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
                        updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
                    )
                )

    invoices = list(invoice_map.values())
    return invoices


async def update_invoice(
    session: AsyncSession, invoice_number: str, user_id: str, invoice_data: InvoiceUpdate
) -> Optional[InvoiceInDB]:
    update_fields = invoice_data.model_dump(exclude_unset=True)
    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    if "total_amount" in update_fields:
        update_fields["total_amount"] = float(update_fields["total_amount"])
    if "invoice_date" in update_fields and update_fields["invoice_date"]:
        update_fields["invoice_date"] = update_fields["invoice_date"].isoformat()
    if "due_date" in update_fields and update_fields["due_date"]:
        update_fields["due_date"] = update_fields["due_date"].isoformat()

    set_clauses = [f"i.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (c:Customer {{user_id: $user_id}})-[:HAS_INVOICE]->(i:Invoice {{invoice_number: $invoice_number}})
    WHERE $book_id IS NULL OR i.book_id = $book_id
    SET {set_query_part}
    RETURN i
    """

    params = {"invoice_number": invoice_number, "user_id": user_id, **update_fields}
    result = await _run(session, query, params)
    record = await result.single()

    if record:
        return await get_invoice_by_number(session, invoice_number, user_id)
    return None


async def delete_invoice(session: AsyncSession, invoice_number: str, user_id: str) -> bool:
    query = """
    MATCH (c:Customer {user_id: $user_id})-[:HAS_INVOICE]->(i:Invoice {invoice_number: $invoice_number})
    WHERE $book_id IS NULL OR i.book_id = $book_id
    OPTIONAL MATCH (i)-[:HAS_ITEM]->(ii:InvoiceItem)
    DETACH DELETE i, ii
    """
    result = await _run(session, query, invoice_number=invoice_number, user_id=user_id)
    return result.consume().counters.nodes_deleted > 0


async def record_payment_for_invoice(
    session: AsyncSession,
    invoice_number: str,
    user_id: str,
    payment_amount: Decimal,
    payment_date: datetime,
    jwt_token: str,
) -> CreateJournalEntryResponse:
    invoice = await get_invoice_by_number(session, invoice_number, user_id)
    if not invoice:
        raise ValidationError(
            detail=f"Invoice {invoice_number} not found.", code="INVOICE_NOT_FOUND"
        )  # MODIFIED: Raise ValidationError instead of ValueError
    if invoice.status == "paid":
        raise ValidationError(
            detail="Invoice already marked as paid.", code="INVOICE_ALREADY_PAID"
        )  # MODIFIED: Raise ValidationError

    # Prepare Journal Entry for payment
    je_lines = [
        # Debit Cash/Bank Account
        JournalLineBase(
            account_number="1010",
            debit=payment_amount,
            credit=Decimal("0.00"),
            description=f"Payment received for Invoice {invoice_number}",
        ),
        # Credit Accounts Receivable
        JournalLineBase(
            account_number="1200",
            debit=Decimal("0.00"),
            credit=payment_amount,
            description=f"Accounts Receivable cleared for Invoice {invoice_number}",
        ),
    ]
    je_description = f"Payment received for Invoice {invoice_number} from {invoice.customer_id}"

    journal_entry = JournalEntryCreate(
        entry_date=payment_date,
        description=je_description,
        reference_number=f"PAY-${invoice_number}",
        source_module="Invoicing",
        lines=je_lines,
    )

    # Send to Accounting Service via API Gateway
    headers = {"Authorization": f"Bearer {jwt_token}", "Content-Type": "application/json"}
    async with httpx.AsyncClient() as client:
        response = await client.post(
            f"{API_GATEWAY_URL}/journal-entries/", headers=headers, json=journal_entry.model_dump(by_alias=True)
        )

    if response.status_code == 201:
        # Update invoice status if JE successful
        await update_invoice(session, invoice_number, user_id, InvoiceUpdate(status="paid"))
        return CreateJournalEntryResponse(
            status="success",
            message=f"Payment recorded and Journal Entry created for Invoice {invoice_number}.",
            journal_entry_id=response.json().get("id"),
        )
    else:
        return CreateJournalEntryResponse(
            status="failed", message=f"Failed to create journal entry for payment: {response.text}"
        )
