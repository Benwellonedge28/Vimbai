"""
Vimbai Business Documents Service
Document generation for financial reports, invoices, receipts, and statements.
Port: 8349
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "business-documents-service"
PORT = int(os.getenv("PORT", "8349"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Business Documents Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class DocumentRequest(BaseModel):
    company_id: str; document_type: str  # invoice, receipt, statement, report, letter
    company_name: str = ""; recipient: str = ""; recipient_address: str = ""
    reference_number: str = Field(default_factory=lambda: str(uuid.uuid4())[:8].upper())
    date: str = Field(default_factory=lambda: datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    line_items: List[Dict] = []  # [{"description": "...", "quantity": 1, "unit_price": 100, "tax_rate": 0.15}]
    notes: str = ""
    payment_terms: str = "Net 30"

class DocumentResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; document_type: str; reference_number: str
    date: str; company_name: str; recipient: str
    subtotal: float; tax_total: float; grand_total: float
    line_items: List[Dict] = []; notes: str = ""
    payment_terms: str = ""

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/generate", response_model=DocumentResult)
async def generate_document(req: DocumentRequest):
    subtotal = 0; tax_total = 0
    items = []
    
    for item in req.line_items:
        qty = float(item.get("quantity", 1))
        unit_price = float(item.get("unit_price", 0))
        tax_rate = float(item.get("tax_rate", 0))
        line_total = qty * unit_price
        line_tax = line_total * tax_rate
        subtotal += line_total
        tax_total += line_tax
        
        items.append({
            "description": item.get("description", ""),
            "quantity": qty, "unit_price": unit_price,
            "line_total": round(line_total, 2),
            "tax": round(line_tax, 2)
        })
    
    grand_total = subtotal + tax_total
    
    return DocumentResult(
        company_id=req.company_id, document_type=req.document_type,
        reference_number=req.reference_number, date=req.date,
        company_name=req.company_name, recipient=req.recipient,
        subtotal=round(subtotal, 2), tax_total=round(tax_total, 2),
        grand_total=round(grand_total, 2), line_items=items,
        notes=req.notes, payment_terms=req.payment_terms
    )

@app.post("/generate/batch", response_model=List[DocumentResult])
async def generate_batch(company_id: str, documents: List[DocumentRequest]):
    results = []
    for doc in documents:
        doc.company_id = company_id
        result = await generate_document(doc)
        results.append(result)
    return results

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
