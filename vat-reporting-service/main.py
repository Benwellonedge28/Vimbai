"""
VAT Reporting Service
Port: 8295
VAT compliance and reporting
"""
import httpx
import structlog
from typing import Any, Dict, List
from pydantic import BaseModel
from fastapi import FastAPI

logger = structlog.get_logger()
app = FastAPI(title="VAT Reporting Service", version="1.0.0")

class VATReportingRequest(BaseModel):
    company_id: str
    vat_sales: float
    vat_purchases: float
    vat_rate: float

class VATReportingResponse(BaseModel):
    company_id: str
    vat_summary: Dict[str, float]
    net_vat_payable: float

@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "vat-reporting", "version": "1.0.0"}

@app.post("/report", response_model=VATReportingResponse)
async def report_vat(request: VATReportingRequest):
    logger.info("Reporting VAT", company=request.company_id)

    output_vat = request.vat_sales * request.vat_rate
    input_vat = request.vat_purchases * request.vat_rate
    net_vat = output_vat - input_vat
    
    return VATReportingResponse(company_id=request.company_id, vat_summary={"output_vat": output_vat, "input_vat": input_vat, "vat_rate": request.vat_rate}, net_vat_payable=net_vat)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8295)
