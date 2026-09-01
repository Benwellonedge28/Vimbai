"""Vimbai R&D Tax Service - Research and development tax incentives. Port: 8357"""
import os, uuid
from datetime import datetime, timezone
from typing import Any, Dict, List
from collections import defaultdict
import structlog
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "r-and-d-tax-service"
PORT = int(os.getenv("PORT", "8357"))
structlog.configure(processors=[structlog.stdlib.add_log_level, structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()], wrapper_class=structlog.stdlib.BoundLogger, logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True)
logger = structlog.get_logger(SERVICE_NAME)
app = FastAPI(title="Vimbai R&D Tax Service", version="2.0.0", docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
try:
    from shared.tracing import setup_tracing; TRACER = setup_tracing(service_name="r-and-d-tax-service", instrument_app=app)
except ImportError:
    TRACER = None

class RDProject(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    project_name: str
    description: str = ""
    start_date: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    end_date: Optional[datetime] = None
    qualified_expenses: float = 0
    deduction_rate: float = 100.0  # 100% deduction for R&D
    tax_savings: float = 0
    corporate_rate: float = 25.0
    status: str = "active"

_projects: Dict[str, List[RDProject]] = defaultdict(list)

@app.get("/")
async def health(): return {"status": "healthy", "service": SERVICE_NAME}

@app.post("/projects", response_model=RDProject)
async def create_project(project: RDProject):
    project.tax_savings = project.qualified_expenses * (project.deduction_rate / 100) * (project.corporate_rate / 100)
    _projects[project.company_id].append(project)
    logger.info("rd_project_created", company_id=project.company_id, name=project.project_name, savings=project.tax_savings)
    return project

@app.get("/projects/{company_id}")
async def get_projects(company_id: str):
    return {"company_id": company_id, "projects": _projects.get(company_id, []), "total": len(_projects.get(company_id, []))}

@app.get("/savings/{company_id}")
async def total_savings(company_id: str):
    projects = _projects.get(company_id, [])
    total_expenses = sum(p.qualified_expenses for p in projects)
    total_savings = sum(p.tax_savings for p in projects)
    return {"company_id": company_id, "total_projects": len(projects), "total_qualified_expenses": total_expenses, "total_tax_savings": total_savings}

if __name__ == "__main__":
    import uvicorn; uvicorn.run(app, host="0.0.0.0", port=PORT)
