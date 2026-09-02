"""
Vimbai Intercompany Service
Manages intercompany transactions, transfer pricing, and eliminations.
"""

import os
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "intercompany-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8350"))

structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_logger_name,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="Vimbai Intercompany Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(
    CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"]
)

try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


class IntercompanyEntity(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    name: str
    legal_entity_code: str
    tax_jurisdiction: str = ""
    currency: str = "USD"
    status: str = "active"
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class IntercompanyTransaction(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    from_entity_id: str
    to_entity_id: str
    transaction_type: str  # loan, service_fee, royalty, sale, cost_allocation
    amount: float
    currency: str = "USD"
    description: str = ""
    transfer_price_basis: str = "cost_plus"  # cost_plus, market, negotiated
    transaction_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    status: str = "pending"  # pending, matched, eliminated
    matched_transaction_id: Optional[str] = None


class EliminationEntry(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    pair_id: str  # links to the matched pair
    debit_entity_id: str
    credit_entity_id: str
    amount: float
    description: str = ""
    elimination_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


entities: List[IntercompanyEntity] = []
transactions: List[IntercompanyTransaction] = []
eliminations: List[EliminationEntry] = []


@app.get("/")
@app.get("/health")
async def health_check():
    return {"status": "healthy", "service": SERVICE_NAME, "version": SERVICE_VERSION}


@app.post("/entities", response_model=IntercompanyEntity)
async def create_entity(name: str, legal_entity_code: str, tax_jurisdiction: str = "", currency: str = "USD"):
    """Register an intercompany entity."""
    entity = IntercompanyEntity(
        name=name,
        legal_entity_code=legal_entity_code,
        tax_jurisdiction=tax_jurisdiction,
        currency=currency,
    )
    entities.append(entity)
    logger.info("Intercompany entity created", entity_id=entity.id, name=name)
    return entity


@app.get("/entities", response_model=List[IntercompanyEntity])
async def list_entities():
    """List all intercompany entities."""
    return entities


@app.post("/transactions", response_model=IntercompanyTransaction)
async def create_transaction(
    from_entity_id: str,
    to_entity_id: str,
    transaction_type: str,
    amount: float,
    currency: str = "USD",
    description: str = "",
    transfer_price_basis: str = "cost_plus",
):
    """Create an intercompany transaction."""
    valid_types = ["loan", "service_fee", "royalty", "sale", "cost_allocation"]
    if transaction_type not in valid_types:
        raise HTTPException(status_code=400, detail=f"Invalid type. Must be one of {valid_types}")

    txn = IntercompanyTransaction(
        from_entity_id=from_entity_id,
        to_entity_id=to_entity_id,
        transaction_type=transaction_type,
        amount=amount,
        currency=currency,
        description=description,
        transfer_price_basis=transfer_price_basis,
    )
    transactions.append(txn)
    logger.info("Intercompany transaction created", txn_id=txn.id, amount=amount)
    return txn


@app.get("/transactions", response_model=List[IntercompanyTransaction])
async def list_transactions(
    from_entity: Optional[str] = None, to_entity: Optional[str] = None, status: Optional[str] = None
):
    """List intercompany transactions."""
    result = transactions
    if from_entity:
        result = [t for t in result if t.from_entity_id == from_entity]
    if to_entity:
        result = [t for t in result if t.to_entity_id == to_entity]
    if status:
        result = [t for t in result if t.status == status]
    return result


@app.post("/transactions/match")
async def match_transactions(txn1_id: str, txn2_id: str):
    """Match two intercompany transactions for elimination."""
    txn1 = next((t for t in transactions if t.id == txn1_id), None)
    txn2 = next((t for t in transactions if t.id == txn2_id), None)
    if not txn1 or not txn2:
        raise HTTPException(status_code=404, detail="Transaction not found")

    if txn1.amount != txn2.amount:
        raise HTTPException(status_code=400, detail="Amounts do not match")

    txn1.status = "matched"
    txn2.status = "matched"
    txn1.matched_transaction_id = txn2_id
    txn2.matched_transaction_id = txn1_id

    elimination = EliminationEntry(
        pair_id=f"{txn1_id}:{txn2_id}",
        debit_entity_id=txn1.from_entity_id,
        credit_entity_id=txn2.from_entity_id,
        amount=txn1.amount,
        description=f"Elimination: {txn1.description}",
    )
    eliminations.append(elimination)

    logger.info("Transactions matched and eliminated", txn1=txn1_id, txn2=txn2_id, amount=txn1.amount)
    return {"matched": True, "elimination_id": elimination.id}


@app.get("/eliminations", response_model=List[EliminationEntry])
async def list_eliminations():
    """List elimination entries."""
    return eliminations


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
