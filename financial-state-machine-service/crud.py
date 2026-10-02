"""
Financial State Machine Service CRUD Operations

Documents move from the in-memory _documents dict to Neo4j:
:FinancialDocument nodes via :OWNS_DOCUMENT edges, book_id stamped,
Book-gated. The state machine itself is unchanged: the TRANSITIONS
map is a pure declaration and transitions are validated in Python
before being persisted. Transition history rides on the document
node as a JSON prop (the established pattern for nested collections)
and is appended in Python on each transition.
"""

import json
from datetime import datetime, timezone
from typing import Any, Dict, Optional

from financial_state_machine_service.dependencies import book_id_var
from financial_state_machine_service.exceptions import NotFoundError, ValidationError
from financial_state_machine_service.models import TRANSITIONS, DocumentState, FinancialDocument, StateTransition
from neo4j import AsyncSession

BOOK_FILTER = "WHERE ($book_id IS NULL OR x.book_id = $book_id)"


async def _run(session, query, params=None, **kw):
    """Run a Cypher query with the Book context parameter always bound."""
    merged = dict(params or {})
    merged.update(kw)
    merged.setdefault("book_id", book_id_var.get())
    return await session.run(query, merged)


def _coerce_dt(value: Any) -> datetime:
    if isinstance(value, datetime):
        return value
    if value is None:
        return datetime.now(timezone.utc)
    if hasattr(value, "iso_format"):
        try:
            return datetime.fromisoformat(value.iso_format())
        except (TypeError, ValueError):
            return datetime.now(timezone.utc)
    if isinstance(value, str):
        try:
            return datetime.fromisoformat(value)
        except ValueError:
            return datetime.now(timezone.utc)
    return datetime.now(timezone.utc)


def _doc_from_node(n: Dict[str, Any]) -> FinancialDocument:
    history = n.get("history", "[]")
    if isinstance(history, str):
        try:
            history = json.loads(history)
        except ValueError:
            history = []
    transitions = [StateTransition(**t) for t in history] if history else []
    return FinancialDocument(
        id=n["id"],
        company_id=n["company_id"],
        document_type=n.get("document_type", "invoice"),
        reference=n.get("reference", ""),
        current_state=n.get("current_state", DocumentState.DRAFT.value),
        history=transitions,
        created_at=_coerce_dt(n.get("created_at")),
    )


async def create_document(session: AsyncSession, user_id: str, doc: FinancialDocument) -> FinancialDocument:
    query = f"""
    MATCH (u:User {{id: $user_id}})
    CREATE (x:FinancialDocument {{
        id: $id,
        user_id: $user_id,
        book_id: $book_id,
        company_id: $company_id,
        document_type: $document_type,
        reference: $reference,
        current_state: $current_state,
        history: $history,
        created_at: datetime($created_at)
    }})
    CREATE (u)-[:OWNS_DOCUMENT]->(x)
    """
    await _run(
        session,
        query,
        id=doc.id,
        user_id=user_id,
        company_id=doc.company_id,
        document_type=doc.document_type,
        reference=doc.reference,
        current_state=(
            doc.current_state.value if isinstance(doc.current_state, DocumentState) else str(doc.current_state)
        ),
        history="[]",
        created_at=doc.created_at.isoformat(),
    )
    return doc


async def find_document(session: AsyncSession, user_id: str, doc_id: str) -> Optional[FinancialDocument]:
    """Return the document if the caller owns it and it is visible in this Book."""
    query = f"""
    MATCH (u:User {{id: $user_id}})-[:OWNS_DOCUMENT]->(x:FinancialDocument {{id: $doc_id}})
    {BOOK_FILTER}
    RETURN x
    """
    result = await _run(session, query, user_id=user_id, doc_id=doc_id)
    records = [r async for r in result]
    if not records:
        return None
    return _doc_from_node(dict(records[0]["x"]))


async def _persist_document(session: AsyncSession, doc: FinancialDocument) -> None:
    query = """
    MATCH (x:FinancialDocument {id: $doc_id})
    SET x.current_state = $current_state,
        x.history = $history
    """
    await _run(
        session,
        query,
        doc_id=doc.id,
        current_state=(
            doc.current_state.value if isinstance(doc.current_state, DocumentState) else str(doc.current_state)
        ),
        history=json.dumps([t.model_dump(mode="json") for t in doc.history]),
    )


async def apply_transition(
    session: AsyncSession,
    user_id: str,
    doc_id: str,
    to_state: DocumentState,
    actor_id: str = "",
    notes: str = "",
) -> Dict[str, Any]:
    """Validate and apply a state transition to a caller-owned, Book-visible document."""
    doc = await find_document(session, user_id, doc_id)
    if doc is None:
        raise NotFoundError("Document not found")
    allowed = TRANSITIONS.get(doc.current_state, [])
    if to_state not in allowed:
        raise ValidationError(
            f"Invalid transition: {doc.current_state.value} -> {to_state.value}. "
            f"Allowed: {[s.value for s in allowed]}"
        )
    record = StateTransition(
        document_id=doc_id,
        from_state=doc.current_state,
        to_state=to_state,
        user_id=actor_id,
        notes=notes,
    )
    doc.history.append(record)
    doc.current_state = to_state
    await _persist_document(session, doc)
    return {"doc_id": doc_id, "current_state": doc.current_state.value, "history_count": len(doc.history)}
