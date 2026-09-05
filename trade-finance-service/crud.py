"""
Trade Finance Service CRUD Operations

Neo4j-backed persistence for trade instruments (letters of credit,
documentary collections, bank guarantees, advance payments, factoring).
All records are stamped with book_id; every read applies the Book
filter `WHERE ($book_id IS NULL OR x.book_id = $book_id)`.
"""

import uuid
from typing import Any, Dict, List, Optional

from neo4j import AsyncSession
from trade_finance_service.dependencies import book_id_var
from trade_finance_service.exceptions import NotFoundError
from trade_finance_service.models import (
    InstrumentResult,
    InstrumentType,
    TradeInstrument,
    TradeInstrumentCreate,
)

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"

FEE_RATES = {
    InstrumentType.LETTER_OF_CREDIT: 0.002,
    InstrumentType.DOCUMENTARY_COLLECTION: 0.001,
    InstrumentType.BANK_GUARANTEE: 0.015,
    InstrumentType.ADVANCE_PAYMENT: 0.0,
    InstrumentType.FACTORING: 0.03,
}


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def estimate_fee(itype: InstrumentType, amount: float) -> float:
    return amount * FEE_RATES.get(itype, 0.0)


def _instrument_from_node(n: Dict[str, Any], user_id: str) -> TradeInstrument:
    return TradeInstrument(
        id=n["id"],
        user_id=user_id,
        book_id=n.get("book_id"),
        company_id=n["company_id"],
        instrument_type=n["instrument_type"],
        counterparty=n["counterparty"],
        amount=float(n.get("amount", 0)),
        currency=n.get("currency", "USD"),
        issue_date=n.get("issue_date", ""),
        expiry_date=n.get("expiry_date", ""),
        status=n.get("status", "issued"),
        issuing_bank=n.get("issuing_bank", ""),
        confirming_bank=n.get("confirming_bank", ""),
    )


def build_result(inst: TradeInstrument) -> InstrumentResult:
    fee = estimate_fee(inst.instrument_type, inst.amount)
    risk = "low" if inst.amount < 100000 else "medium" if inst.amount < 500000 else "high"
    docs = ["Commercial invoice", "Bill of lading", "Certificate of origin", "Packing list"]
    if inst.instrument_type == InstrumentType.LETTER_OF_CREDIT:
        docs.extend(["LC application", "Proforma invoice"])
    return InstrumentResult(
        id=inst.id,
        company_id=inst.company_id,
        instrument_type=inst.instrument_type.value,
        amount=inst.amount,
        fee_estimate=round(fee, 2),
        status=inst.status,
        risk_assessment=risk,
        documentation_required=docs,
    )


async def create_instrument(session: AsyncSession, user_id: str, payload: TradeInstrumentCreate) -> TradeInstrument:
    inst_id = str(uuid.uuid4())
    from datetime import datetime, timezone

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (x:TradeInstrument {
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        instrument_type: $instrument_type,
        counterparty: $counterparty,
        amount: toFloat($amount),
        currency: $currency,
        issue_date: $issue_date,
        expiry_date: $expiry_date,
        status: 'issued',
        issuing_bank: $issuing_bank,
        confirming_bank: $confirming_bank
    })
    CREATE (u)-[:OWNS_INSTRUMENT]->(x)
    RETURN x
    """
    params = {
        "id": inst_id,
        "user_id": user_id,
        "company_id": payload.company_id,
        "instrument_type": payload.instrument_type.value,
        "counterparty": payload.counterparty,
        "amount": payload.amount,
        "currency": payload.currency,
        "issue_date": datetime.now(timezone.utc).strftime("%Y-%m-%d"),
        "expiry_date": payload.expiry_date,
        "issuing_bank": payload.issuing_bank,
        "confirming_bank": payload.confirming_bank,
    }
    result = await _run(session, query, params)
    records = [r async for r in result]
    return _instrument_from_node(dict(records[0]["x"]), user_id)


async def list_instruments(
    session: AsyncSession, user_id: str, company_id: str, status: Optional[str] = ""
) -> List[TradeInstrument]:
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_INSTRUMENT]->(x:TradeInstrument {{company_id: $company_id}})
    {BOOK_FILTER}
    RETURN x
    ORDER BY x.issue_date ASC, x.id ASC
    """
    result = await _run(session, query, user_id=user_id, company_id=company_id)
    items = [_instrument_from_node(dict(r["x"]), user_id) async for r in result]
    if status:
        items = [i for i in items if i.status == status]
    return items


async def _set_status(session: AsyncSession, user_id: str, company_id: str, instrument_id: str, new_status: str):
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_INSTRUMENT]->(x:TradeInstrument {{company_id: $company_id, id: $instrument_id}})
    {BOOK_FILTER}
    SET x.status = $new_status
    RETURN x
    """
    result = await _run(
        session, query, user_id=user_id, company_id=company_id, instrument_id=instrument_id, new_status=new_status
    )
    records = [r async for r in result]
    if not records:
        raise NotFoundError("Instrument not found")
    return _instrument_from_node(dict(records[0]["x"]), user_id)


async def present_documents(
    session: AsyncSession, user_id: str, company_id: str, instrument_id: str
) -> TradeInstrument:
    return await _set_status(session, user_id, company_id, instrument_id, "presented")


async def settle_instrument(
    session: AsyncSession, user_id: str, company_id: str, instrument_id: str
) -> TradeInstrument:
    return await _set_status(session, user_id, company_id, instrument_id, "paid")
