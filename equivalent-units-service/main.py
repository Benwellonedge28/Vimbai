"""
Vimbai Equivalent Units Service
Process costing equivalent units of production calculation.
Port: 8384
"""
import os, uuid
from typing import Dict, List
import structlog
from pydantic import BaseModel, Field
from fastapi import FastAPI

SERVICE_NAME = "equivalent-units-service"
PORT = int(os.getenv("PORT", "8384"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()])
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai Equivalent Units Service", version="2.0.0", docs_url="/docs")
try:
    from shared.tracing import setup_tracing; setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    pass

class EquivalentUnitsRequest(BaseModel):
    company_id: str; department: str; period: str
    method: str = "weighted_average"  # weighted_average, fifo
    units_started: int; units_completed: int
    ending_wip_units: int; ending_wip_completion_materials: float = 0.0
    ending_wip_completion_conversion: float = 0.0
    beginning_wip_units: int = 0; beginning_wip_completion_materials: float = 0.0
    beginning_wip_completion_conversion: float = 0.0
    materials_cost_beginning: float = 0; conversion_cost_beginning: float = 0
    materials_cost_added: float = 0; conversion_cost_added: float = 0

class EquivalentUnitsResult(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str; department: str; period: str; method: str
    equivalent_units_materials: float; equivalent_units_conversion: float
    cost_per_unit_materials: float; cost_per_unit_conversion: float
    cost_per_unit_total: float
    cost_of_completed: float; cost_of_ending_wip: float

@app.get("/")
@app.get("/health")
async def health():
    return {"status": "healthy", "service": SERVICE_NAME, "version": "2.0.0"}

@app.post("/calculate", response_model=EquivalentUnitsResult)
async def calculate_equivalent_units(req: EquivalentUnitsRequest):
    if req.method == "weighted_average":
        eu_materials = req.units_completed + req.ending_wip_units * req.ending_wip_completion_materials
        eu_conversion = req.units_completed + req.ending_wip_units * req.ending_wip_completion_conversion
        
        total_materials_cost = req.materials_cost_beginning + req.materials_cost_added
        total_conversion_cost = req.conversion_cost_beginning + req.conversion_cost_added
        
        cpu_materials = total_materials_cost / eu_materials if eu_materials else 0
        cpu_conversion = total_conversion_cost / eu_conversion if eu_conversion else 0
        
    else:  # FIFO
        eu_materials = (req.units_completed - req.beginning_wip_units * req.beginning_wip_completion_materials) + \
                       req.ending_wip_units * req.ending_wip_completion_materials
        eu_conversion = (req.units_completed - req.beginning_wip_units * req.beginning_wip_completion_conversion) + \
                        req.ending_wip_units * req.ending_wip_completion_conversion
        
        cpu_materials = req.materials_cost_added / eu_materials if eu_materials else 0
        cpu_conversion = req.conversion_cost_added / eu_conversion if eu_conversion else 0
    
    cpu_total = cpu_materials + cpu_conversion
    
    cost_completed = req.units_completed * cpu_total
    cost_ending_wip = (req.ending_wip_units * req.ending_wip_completion_materials * cpu_materials +
                      req.ending_wip_units * req.ending_wip_completion_conversion * cpu_conversion)
    
    return EquivalentUnitsResult(
        company_id=req.company_id, department=req.department,
        period=req.period, method=req.method,
        equivalent_units_materials=round(eu_materials, 1),
        equivalent_units_conversion=round(eu_conversion, 1),
        cost_per_unit_materials=round(cpu_materials, 4),
        cost_per_unit_conversion=round(cpu_conversion, 4),
        cost_per_unit_total=round(cpu_total, 4),
        cost_of_completed=round(cost_completed, 2),
        cost_of_ending_wip=round(cost_ending_wip, 2)
    )

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
