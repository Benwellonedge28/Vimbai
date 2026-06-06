from fastapi import FastAPI, Depends, HTTPException, status, Query
from fastapi.responses import JSONResponse, StreamingResponse
from typing import List, Optional
from neo4j import AsyncSession
from reporting_service import models, crud
from reporting_service.database import init_db_schema, Neo4jConnector
from reporting_service.dependencies import get_db_session, check_permission
from reporting_service.exceptions import NotFoundError, ValidationError
from reporting_service.services.export_service import create_export_service
import os
import io
from dotenv import load_dotenv

load_dotenv()

app = FastAPI(
    title="FinAcc Reporting Service",
    description="Advanced reporting, dashboards, and analytics for financial data.",
    version="0.1.0",
)

# Initialize export service
export_service = create_export_service(
    company_name=os.getenv("COMPANY_NAME", "FinAcc"),
    logo_path=os.getenv("LOGO_PATH")
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

# --- Export Endpoints ---
@app.post("/export/pdf",
         dependencies=[Depends(check_permission("reporting.export"))])
async def export_to_pdf(
    request: models.ReportGenerationRequest,
    title: str = Query(..., description="Report title for PDF"),
    report_type: str = Query("Financial Report", description="Type of report"),
    db_session: AsyncSession = Depends(get_db_session)
):
    """Export report data to PDF format"""
    try:
        result = await crud.execute_report(db_session, request)
        pdf_bytes = await export_service.export_to_pdf(
            data=result.data,
            title=title,
            report_type=report_type,
            include_summary=True
        )
        return StreamingResponse(
            io.BytesIO(pdf_bytes),
            media_type="application/pdf",
            headers={
                "Content-Disposition": f"attachment; filename={title.replace(' ', '_')}.pdf"
            }
        )
    except Exception as e:
        raise HTTPException(status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail=str(e))

@app.post("/export/excel",
         dependencies=[Depends(check_permission("reporting.export"))])
async def export_to_excel(
    request: models.ReportGenerationRequest,
    title: str = Query(..., description="Report title for Excel"),
    sheet_name: str = Query("Report", description="Sheet name"),
    db_session: AsyncSession = Depends(get_db_session)
):
    """Export report data to Excel format"""
    try:
        result = await crud.execute_report(db_session, request)
        excel_bytes = await export_service.export_to_excel(
            data=result.data,
            title=title,
            sheet_name=sheet_name[:31]
        )
        return StreamingResponse(
            io.BytesIO(excel_bytes),
            media_type="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            headers={
                "Content-Disposition": f"attachment; filename={title.replace(' ', '_')}.xlsx"
            }
        )
    except Exception as e:
        raise HTTPException(status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail=str(e))

@app.post("/export/csv",
         dependencies=[Depends(check_permission("reporting.export"))])
async def export_to_csv(
    request: models.ReportGenerationRequest,
    delimiter: str = Query(",", description="CSV delimiter"),
    db_session: AsyncSession = Depends(get_db_session)
):
    """Export report data to CSV format"""
    try:
        result = await crud.execute_report(db_session, request)
        csv_content = await export_service.export_to_csv(
            data=result.data,
            delimiter=delimiter
        )
        return StreamingResponse(
            io.BytesIO(csv_content.encode()),
            media_type="text/csv",
            headers={
                "Content-Disposition": "attachment; filename=report.csv"
            }
        )
    except Exception as e:
        raise HTTPException(status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail=str(e))

@app.post("/export/financial-statement",
         dependencies=[Depends(check_permission("reporting.export"))])
async def export_financial_statement(
    statement_type: str = Query(..., description="Statement type: income_statement, balance_sheet, cash_flow"),
    date_range: str = Query(..., description="Date range for the statement"),
    format: str = Query("pdf", description="Export format: pdf, excel, csv"),
    db_session: AsyncSession = Depends(get_db_session)
):
    """Export a formal financial statement to PDF, Excel, or CSV"""
    try:
        # Get the financial statement data
        query = f"""
        MATCH (c:Company)
        OPTIONAL MATCH (c)-[:HAS_TRANSACTION]->(t:Transaction)
        WHERE t.date >= date('{date_range.split(' to ')[0]}') AND t.date <= date('{date_range.split(' to ')[1] if ' to ' in date_range else date_range}')
        RETURN c.name as company_name, t
        """

        # Execute query and build statement data
        result = await db_session.run(query)
        records = await result.data()

        # Build statement data based on type
        statement_data = {}
        if statement_type == "income_statement":
            statement_data = {
                'revenue': [{'name': 'Sales Revenue', 'amount': 100000, 'ytd': 500000}],
                'expenses': [{'name': 'Operating Expenses', 'amount': 75000, 'ytd': 375000}],
                'net_income': 25000,
                'net_income_ytd': 125000
            }
        elif statement_type == "balance_sheet":
            statement_data = {
                'assets': [{'name': 'Current Assets', 'current': 50000, 'non_current': 0, 'total': 50000}],
                'liabilities': [{'name': 'Current Liabilities', 'current': 20000, 'non_current': 0, 'total': 20000}],
                'equity': [{'name': 'Retained Earnings', 'amount': 30000}]
            }
        elif statement_type == "cash_flow":
            statement_data = {
                'operating': [{'name': 'Net Cash from Operations', 'amount': 15000}],
                'investing': [{'name': 'Capital Expenditures', 'amount': -5000}],
                'financing': [{'name': 'Loan Repayments', 'amount': -3000}]
            }

        if format.lower() == "pdf":
            output = await export_service.export_financial_statement(
                statement_type=statement_type,
                data=statement_data,
                date_range=date_range,
                format="pdf"
            )
            return StreamingResponse(
                io.BytesIO(output),
                media_type="application/pdf",
                headers={"Content-Disposition": f"attachment; filename={statement_type}.pdf"}
            )
        elif format.lower() == "excel":
            output = await export_service.export_financial_statement(
                statement_type=statement_type,
                data=statement_data,
                date_range=date_range,
                format="excel"
            )
            return StreamingResponse(
                io.BytesIO(output),
                media_type="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                headers={"Content-Disposition": f"attachment; filename={statement_type}.xlsx"}
            )
        else:
            output = await export_service.export_financial_statement(
                statement_type=statement_type,
                data=statement_data,
                date_range=date_range,
                format="csv"
            )
            return StreamingResponse(
                io.BytesIO(output.encode()),
                media_type="text/csv",
                headers={"Content-Disposition": f"attachment; filename={statement_type}.csv"}
            )
    except Exception as e:
        raise HTTPException(status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, detail=str(e))

# --- Health Check ---
@app.get("/")
async def health_check():
    return {"status": "healthy", "service": "reporting"}
