"""
Vimbai Suspense Error Service
Identifies, classifies, and resolves suspense account errors.
Port: 8346
"""
import os, uuid
from datetime import datetime, timezone
from typing import Dict, List, Optional
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "suspense-error-service"
PORT = int(os.getenv("PORT", "8346"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Suspense Error Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class SuspenseError(BaseModel):
    error_type: str  # omission, commission, principle, original_entry, compensating, complete_reversal
    description: str; amount: float; account_affected: str
    correct_debit: Optional[str] = None; correct_credit: Optional[str] = None

class SuspenseRequest(BaseModel):
    company_id: str; period: str; suspense_balance: float; errors: List[SuspenseError]

class SuspenseResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; period: str
    original_suspense_balance: float; total_errors_found: int
    corrected_suspense_balance: float; corrections: List[Dict] = []

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/analyze", response_model=SuspenseResult)
async def analyze_suspense(req: SuspenseRequest):
    total_corrections = 0
    corrections = []
    
    for error in req.errors:
        correction_amount = error.amount
        if error.error_type == "omission":
            effect = correction_amount
            action = f"Record missing entry: Dr {error.correct_debit or 'N/A'}, Cr {error.correct_credit or 'N/A'}"
        elif error.error_type == "commission":
            effect = 0  # No effect on suspense
            action = f"Transfer from wrong to correct account"
        elif error.error_type == "principle":
            effect = 0
            action = f"Reclassify from {error.account_affected} to correct category"
        elif error.error_type == "original_entry":
            effect = correction_amount * 2  # Doubled
            action = f"Correct understated/overstated entry by {correction_amount * 2}"
        elif error.error_type == "complete_reversal":
            effect = correction_amount * 2
            action = f"Reverse and re-enter correctly (Dr/Cr swapped)"
        elif error.error_type == "compensating":
            effect = 0
            action = "Offsetting errors - no net suspense effect"
        else:
            effect = correction_amount
            action = f"Correct entry for {error.description}"
        
        total_corrections += effect
        corrections.append({
            "error_type": error.error_type,
            "description": error.description,
            "amount": round(correction_amount, 2),
            "effect_on_suspense": round(effect, 2),
            "account_affected": error.account_affected,
            "corrective_action": action
        })
    
    corrected_balance = req.suspense_balance - total_corrections
    
    return SuspenseResult(
        company_id=req.company_id, period=req.period,
        original_suspense_balance=round(req.suspense_balance, 2),
        total_errors_found=len(req.errors),
        corrected_suspense_balance=round(corrected_balance, 2),
        corrections=corrections
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
