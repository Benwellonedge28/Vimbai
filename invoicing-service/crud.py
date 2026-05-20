from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from invoicing_service.models import (
    CustomerCreate, CustomerUpdate, CustomerInDB,
    InvoiceCreate, InvoiceUpdate, InvoiceInDB,
    InvoiceItemCreate, InvoiceItemInDB,
    JournalEntryCreate, CreateJournalEntryResponse, JournalLineBase
)
from datetime import datetime
import uuid
from decimal import Decimal
import httpx
import os
from invoicing_service.exceptions import ValidationError # NEW

API_GATEWAY_URL = os.getenv("API_GATEWAY_URL", "http://api-gateway:8081")

# ... (Customer CRUD unchanged) ...

# --- Invoice CRUD ---
async def create_invoice(session: AsyncSession, user_id: str, invoice_data: InvoiceCreate) -> InvoiceInDB:
    invoice_neo4j_id = str(uuid.uuid4())
    created_at = datetime.utcnow()
    updated_at = datetime.utcnow()

    # Create Invoice node and link to Customer
    invoice_query = """
    MATCH (c:Customer {customer_id: $customer_id, user_id: $user_id})
    CREATE (i:Invoice {
        id: $id,
        invoice_number: $invoice_number,
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
    invoice_params = invoice_data.model_dump(exclude={
        "items"
    })
    invoice_params["id"] = invoice_neo4j_id
    invoice_params["user_id"] = user_id # Passed for matching customer
    invoice_params["invoice_date"] = invoice_params["invoice_date"].isoformat()
    invoice_params["due_date"] = invoice_params["due_date"].isoformat()
    invoice_params["total_amount"] = float(invoice_params["total_amount"])
    
    result = await session.run(invoice_query, invoice_params)
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
        # created_at and updated_at are set by default factory on the model

        item_result = await session.run(item_query, item_params)
        item_node = (await item_result.single())["ii"]
        invoice_items_in_db.append(InvoiceItemInDB(
            id=item_node["id"],
            description=item_node["description"],
            quantity=Decimal(str(item_node["quantity"])),
            unit_price=Decimal(str(item_node["unit_price"])),
            amount=Decimal(str(item_node["amount"])),
            account_number=item_node["account_number"],
            created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
            updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
        ))
    
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
    OPTIONAL MATCH (i)-[:HAS_ITEM]->(ii:InvoiceItem)
    RETURN i, COLLECT(ii) AS items, c.customer_id AS customer_id
    """
    result = await session.run(query, invoice_number=invoice_number, user_id=user_id)
    record = await result.single()

    if record:
        invoice_node = record["i"]
        items_data = record["items"]
        customer_id = record["customer_id"] # Get customer_id directly from the RETURN clause
        
        invoice_items_in_db = []
        for item_node in items_data:
            if item_node: # Ensure item_node is not None (COLLECT can return [None] if no items)
                invoice_items_in_db.append(InvoiceItemInDB(
                    id=item_node["id"],
                    description=item_node["description"],
                    quantity=Decimal(str(item_node["quantity"])),
                    unit_price=Decimal(str(item_node["unit_price"])),
                    amount=Decimal(str(item_node["amount"])),
                    account_number=item_node["account_number"],
                    created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
                    updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
                ))
        
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
    OPTIONAL MATCH (i)-[:HAS_ITEM]->(ii:InvoiceItem)
    RETURN i, COLLECT(ii) AS items, c.customer_id AS customer_id
    ORDER BY i.invoice_date DESC
    """
    result = await session.run(query, user_id=user_id)

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
                items=[], # Initialize empty list for items
                created_at=datetime.fromisoformat(invoice_node["created_at"].iso_format()),
                updated_at=datetime.fromisoformat(invoice_node["updated_at"].iso_format()),
            )
        
        for item_node in items_data:
            if item_node: # Only add if item_node is not None (for invoices with no items)
                invoice_map[invoice_id].items.append(InvoiceItemInDB(
                    id=item_node["id"],
                    description=item_node["description"],
                    quantity=Decimal(str(item_node["quantity"])),
                    unit_price=Decimal(str(item_node["unit_price"])),
                    amount=Decimal(str(item_node["amount"])),
                    account_number=item_node["account_number"],
                    created_at=datetime.fromisoformat(item_node["created_at"].iso_format()),
                    updated_at=datetime.fromisoformat(item_node["updated_at"].iso_format()),
                ))
    
    invoices = list(invoice_map.values())
    return invoices
    
async def update_invoice(session: AsyncSession, invoice_number: str, user_id: str, invoice_data: InvoiceUpdate) -> Optional[InvoiceInDB]:
    update_fields = invoice_data.model_dump(exclude_unset=True)
    update_fields["updated_at"] = datetime.utcnow().isoformat()
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
    SET {set_query_part}
    RETURN i
    """
    
    params = {"invoice_number": invoice_number, "user_id": user_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()

    if record:
        return await get_invoice_by_number(session, invoice_number, user_id)
    return None

async def delete_invoice(session: AsyncSession, invoice_number: str, user_id: str) -> bool:
    query = """
    MATCH (c:Customer {user_id: $user_id})-[:HAS_INVOICE]->(i:Invoice {invoice_number: $invoice_number})
    OPTIONAL MATCH (i)-[:HAS_ITEM]->(ii:InvoiceItem)
    DETACH DELETE i, ii
    """
    result = await session.run(query, invoice_number=invoice_number, user_id=user_id)
    return result.consume().counters.nodes_deleted > 0

async def record_payment_for_invoice(session: AsyncSession, invoice_number: str, user_id: str, payment_amount: Decimal, payment_date: datetime, jwt_token: str) -> CreateJournalEntryResponse:
    invoice = await get_invoice_by_number(session, invoice_number, user_id)
    if not invoice:
        raise ValidationError(detail=f"Invoice {invoice_number} not found.", code="INVOICE_NOT_FOUND") # MODIFIED: Raise ValidationError instead of ValueError
    if invoice.status == "paid":
        raise ValidationError(detail="Invoice already marked as paid.", code="INVOICE_ALREADY_PAID") # MODIFIED: Raise ValidationError

    # Prepare Journal Entry for payment
    je_lines = [
        # Debit Cash/Bank Account
        JournalLineBase(account_number="1010", debit=payment_amount, credit=Decimal('0.00'), description=f"Payment received for Invoice {invoice_number}"),
        # Credit Accounts Receivable
        JournalLineBase(account_number="1200", debit=Decimal('0.00'), credit=payment_amount, description=f"Accounts Receivable cleared for Invoice {invoice_number}")
    ]
    je_description = f"Payment received for Invoice {invoice_number} from {invoice.customer_id}"
    
    journal_entry = JournalEntryCreate(
        entry_date=payment_date,
        description=je_description,
        reference_number=f"PAY-${invoice_number}",
        source_module="Invoicing",
        lines=je_lines
    )

    # Send to Accounting Service via API Gateway
    headers = {
        "Authorization": f"Bearer {jwt_token}",
        "Content-Type": "application/json"
    }
    async with httpx.AsyncClient() as client:
        response = await client.post(
            f"{API_GATEWAY_URL}/journal-entries/",
            headers=headers,
            json=journal_entry.model_dump(by_alias=True)
        )
    
    if response.status_code == 201:
        # Update invoice status if JE successful
        await update_invoice(session, invoice_number, user_id, InvoiceUpdate(status="paid"))
        return CreateJournalEntryResponse(
            status="success",
            message=f"Payment recorded and Journal Entry created for Invoice {invoice_number}.",
            journal_entry_id=response.json().get("id")
        )
    else:
        return CreateJournalEntryResponse(
            status="failed",
            message=f"Failed to create journal entry for payment: {response.text}"
        )
