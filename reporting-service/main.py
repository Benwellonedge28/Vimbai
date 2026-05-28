from fastapi import FastAPI, Depends, HTTPException, status
from fastapi.responses import JSONResponse
from typing import List, Optional
from neo4j import AsyncSession
from reporting_service import models, crud
from reporting_service.database import init_db_schema, Neo4jConnector
from reporting_service.dependencies import get_db_session, check_permission
from reporting_service.exceptions import NotFoundError, ValidationError
import os
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="FinAcc Reporting Service",
    description="Advanced reporting, dashboards, and analytics for financial data.",
    version="0.1.0",
)

@app.on_event("startup")
async def startup_event():
    Neo4jConnector.configure(
        uri=os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        user=os.getenv("NEO4J_USER", "neo4j"),
        password=os.getenv("NEO4J_PASSWORD", "neo4j")
    )
    Neo4jConnector.get_driver()
    await init_db_schema()

@app.on_event("shutdown")
async def shutdown_event():
    await Neo4jConnector.close_driver()

# --- Exception Handlers ---
@app.exception_handler(NotFoundError)
async def not_found_handler(request, exc: NotFoundError):
    return JSONResponse(status_code=exc.status_code, content=exc.detail)

@app.exception_handler(ValidationError)
async def validation_handler(request, exc: ValidationError):
    return JSONResponse(status_code=exc.status_status, content=exc.detail)

# --- Dashboard Endpoints ---
@app.post("/dashboards/", response_model=models.DashboardInDB, status_code=status.HTTP_201_CREATED,
          dependencies=[Depends(check_permission("reporting.create"))])
async def create_dashboard(
    dashboard: models.DashboardCreate,
    user_id: str = Depends(lambda: "system"),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_dashboard(db_session, user_id, dashboard)

@app.get("/dashboards/", response_model=List[models.DashboardInDB],
         dependencies=[Depends(check_permission("reporting.read"))])
async def list_dashboards(
    user_id: str = Depends(lambda: "system"),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.get_user_dashboards(db_session, user_id)

@app.get("/dashboards/{dashboard_id}", response_model=models.DashboardInDB,
         dependencies=[Depends(check_permission("reporting.read"))])
async def get_dashboard(dashboard_id: str, db_session: AsyncSession = Depends(get_db_session)):
    dashboard = await crud.get_dashboard(db_session, dashboard_id)
    if not dashboard:
        raise NotFoundError(detail="Dashboard not found")
    return dashboard

@app.delete("/dashboards/{dashboard_id}", status_code=status.HTTP_204_NO_CONTENT,
           dependencies=[Depends(check_permission("reporting.delete"))])
async def delete_dashboard(dashboard_id: str, db_session: AsyncSession = Depends(get_db_session)):
    success = await crud.delete_dashboard(db_session, dashboard_id)
    if not success:
        raise NotFoundError(detail="Dashboard not found")
    return {"ok": True}

# --- Report Template Endpoints ---
@app.post("/templates/", response_model=models.ReportTemplateInDB, status_code=status.HTTP_201_CREATED,
         dependencies=[Depends(check_permission("reporting.create"))])
async def create_template(
    template: models.ReportTemplateCreate,
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_report_template(db_session, "system", template)

@app.get("/templates/", response_model=List[models.ReportTemplateInDB],
        dependencies=[Depends(check_permission("reporting.read"))])
async def list_templates(db_session: AsyncSession = Depends(get_db_session)):
    return await crud.get_all_report_templates(db_session)

@app.get("/templates/{template_id}", response_model=models.ReportTemplateInDB,
        dependencies=[Depends(check_permission("reporting.read"))])
async def get_template(template_id: str, db_session: AsyncSession = Depends(get_db_session)):
    template = await crud.get_report_template(db_session, template_id)
    if not template:
        raise NotFoundError(detail="Template not found")
    return template

# --- Report Execution Endpoints ---
@app.post("/reports/execute", response_model=models.ReportResult,
         dependencies=[Depends(check_permission("reporting.execute"))])
async def execute_report(
    request: models.ReportGenerationRequest,
    db_session: AsyncSession = Depends(get_db_session)
):
    try:
        return await crud.execute_report(db_session, request)
    except ValueError as e:
        raise ValidationError(detail=str(e))

# --- Scheduled Reports Endpoints ---
@app.post("/scheduled-reports/", response_model=models.ScheduledReportInDB,
         dependencies=[Depends(check_permission("reporting.schedule"))])
async def create_scheduled_report(
    report: models.ScheduledReportCreate,
    user_id: str = Depends(lambda: "system"),
    db_session: AsyncSession = Depends(get_db_session)
):
    return await crud.create_scheduled_report(db_session, user_id, report)

# --- Health Check ---
@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "reporting"}
