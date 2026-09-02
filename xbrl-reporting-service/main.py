"""
Vimbai XBRL Reporting Service
XBRL/iXBRL taxonomic tagging and electronic filing of financial statements.
Port: 8397
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "xbrl-reporting-service"
PORT = int(os.getenv("PORT", "8397"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai XBRL Reporting Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class FinancialConcept(BaseModel):
    concept: str; value: float; context_ref: str = "current_year"
    unit_ref: str = "USD"; decimals: int = -2

class XBRLReportRequest(BaseModel):
    company_id: str; fiscal_year: int
    entity_name: str; entity_identifier: str
    taxonomy: str = "ifrs-full"  # ifrs-full, us-gaap
    concepts: List[FinancialConcept] = []

class XBRLReportResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; entity_name: str; fiscal_year: int
    taxonomy: str; concept_count: int
    validation_status: str; validation_errors: List[str] = []
    xbrl_facts: List[Dict] = []
    filing_metadata: Dict[str, str] = {}

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/generate", response_model=XBRLReportResult)
async def generate_xbrl(req: XBRLReportRequest):
    errors = []
    facts = []
    
    required_concepts = ["RevenueFromContracts", "ProfitLoss", "Assets", "Liabilities", "Equity"]
    found_concepts = set()
    
    for c in req.concepts:
        found_concepts.add(c.concept)
        facts.append({
            "concept": c.concept,
            "taxonomy_ref": f"{req.taxonomy}:{c.concept}",
            "value": round(c.value, 2),
            "context": c.context_ref,
            "unit": c.unit_ref,
            "decimals": c.decimals
        })
    
    for rc in required_concepts:
        if rc not in found_concepts:
            errors.append(f"Missing required concept: {rc}")
    
    validation = "valid" if not errors else "invalid"
    
    return XBRLReportResult(
        company_id=req.company_id, entity_name=req.entity_name,
        fiscal_year=req.fiscal_year, taxonomy=req.taxonomy,
        concept_count=len(req.concepts), validation_status=validation,
        validation_errors=errors, xbrl_facts=facts,
        filing_metadata={
            "entity_identifier": req.entity_identifier,
            "reporting_period": f"FY{req.fiscal_year}",
            "generated_at": datetime.now(timezone.utc).isoformat(),
            "schema": f"https://xbrl.ifrs.org/taxonomy/{req.taxonomy}/",
            "language": "en"
        }
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
