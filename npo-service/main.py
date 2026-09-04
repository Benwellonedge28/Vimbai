"""
NPO Service - Non-Profit Organization Accounting Service

Comprehensive API for NPO accounting covering all 100 concepts:
- Fund Accounting
- Net Assets Management
- Revenue and Grant Tracking
- Budget and Cost Allocation
- Project and Program Management
- Donor Management
- Compliance and Governance
- Performance and Impact Measurement
"""

import os
import uuid
from datetime import date, datetime
from decimal import Decimal
from typing import Any, Dict, List, Optional

from dotenv import load_dotenv
from fastapi import Depends, FastAPI, HTTPException, Query, Request, status
from fastapi.openapi.utils import get_openapi
from fastapi.responses import JSONResponse
from neo4j import AsyncSession
from npo_service import crud, models
from npo_service.database import Neo4jConnector, init_db_schema
from npo_service.dependencies import book_id_var, get_jwt_token, get_user_id
from npo_service.exceptions import (
    ConflictError,
    ForbiddenError,
    NotFoundError,
    RestrictionViolationError,
    UnauthorizedError,
    ValidationError,
)
from pydantic import ValidationError as PydanticValidationError

load_dotenv()

# =============================================================================
# OPENAPI SCHEMA CONFIGURATION
# =============================================================================


def custom_openapi():
    """Generate custom OpenAPI schema with Vimbai NPO-specific metadata."""
    if app.openapi_schema:
        return app.openapi_schema

    openapi_schema = get_openapi(
        title="Vimbai NPO Service",
        description="""
## Vimbai NPO (Non-Profit Organization) Service API

Comprehensive API for non-profit organization accounting covering fund accounting,
net assets management, grant lifecycle, and impact measurement.

### Features
- **Fund Accounting**: General, restricted, endowment, capital, project funds
- **Net Assets**: With/Without donor restrictions tracking
- **Revenue Management**: Donations, grants, memberships, fundraising
- **Donor Management**: Donor information and contribution history
- **Grant Lifecycle**: Application, approval, drawdowns, reporting
- **Budget Management**: Budget planning and variance analysis
- **Project & Program Tracking**: Resource allocation and metrics
- **Compliance & Governance**: Internal controls and audit reports
- **Impact Measurement**: SROI analysis, volunteer tracking

### NPO Accounting Concepts (100 Total)
1. Fund Accounting (General, Restricted, Endowment, Capital, Project)
2. Net Assets Classification (With/Without Donor Restrictions)
3. Revenue Recognition (Contributions, Grants, Memberships)
4. Asset Management (Tangible, Intangible, Current, Fixed)
5. Liability Tracking (Current, Long-term)
6. Financial Statements (Statement of Activities, Position)
7. Budget Variance Analysis
8. Grant Reporting Requirements
9. Donor Gift Processing
10. Program Cost Allocation

### Authentication
All endpoints require JWT Bearer token authentication.

### Rate Limits
- Default: 1000 requests/minute
- Authenticated: 5000 requests/minute
        """,
        version="1.0.0",
        routes=app.routes,
    )

    # Add security schemes
    openapi_schema["components"]["securitySchemes"] = {
        "BearerAuth": {"type": "http", "scheme": "bearer", "bearerFormat": "JWT", "description": "Enter your JWT token"}
    }

    # Add custom tags for organization
    openapi_schema["tags"] = [
        {"name": "funds", "description": "NPO Fund Accounting - fund creation, transactions, restrictions"},
        {"name": "net-assets", "description": "Net assets classification and tracking"},
        {"name": "donations", "description": "Donation tracking and management"},
        {"name": "grants", "description": "Grant lifecycle management"},
        {"name": "donors", "description": "Donor information and history"},
        {"name": "budgets", "description": "NPO budget planning and variance"},
        {"name": "projects", "description": "NPO project tracking"},
        {"name": "programs", "description": "NPO program management"},
        {"name": "compliance", "description": "Internal controls and audit reports"},
        {"name": "impact", "description": "Impact measurement and SROI"},
        {"name": "volunteers", "description": "Volunteer hours tracking"},
        {"name": "statements", "description": "NPO financial statements"},
        {"name": "assets", "description": "NPO asset management"},
        {"name": "health", "description": "Service health checks"},
    ]

    app.openapi_schema = openapi_schema
    return app.openapi_schema


app = FastAPI(
    title="Vimbai NPO Service",
    description="Non-Profit Organization Accounting Service - Manages fund accounting, net assets, grants, budgets, compliance, and impact measurement.",
    version="0.1.0",
    docs_url="/docs",
    redoc_url="/redoc",
    openapi_url="/openapi.json",
)


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Bind the gateway-verified X-Book-ID into the request-scoped contextvar."""
    token = book_id_var.set(request.headers.get("x-book-id"))
    try:
        return await call_next(request)
    finally:
        book_id_var.reset(token)


# Apply custom OpenAPI schema
app.openapi = custom_openapi


@app.on_event("startup")
async def startup_event():
    Neo4jConnector.configure(
        uri=os.getenv("NEO4J_URI", "bolt://localhost:7687"),
        user=os.getenv("NEO4J_USER", "neo4j"),
        password=os.getenv("NEO4J_PASSWORD", "neo4j"),
    )
    Neo4jConnector.get_driver()
    await init_db_schema()


@app.on_event("shutdown")
async def shutdown_event():
    await Neo4jConnector.close_driver()


# --- Global Exception Handlers ---
@app.exception_handler(NotFoundError)
async def not_found_exception_handler(request, exc: NotFoundError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(ConflictError)
async def conflict_exception_handler(request, exc: ConflictError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(ValidationError)
async def validation_exception_handler(request, exc: ValidationError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(UnauthorizedError)
async def unauthorized_exception_handler(request, exc: UnauthorizedError):
    return JSONResponse(
        status_code=exc.status_code,
        content={"detail": exc.detail, "code": exc.code},
        headers={"WWW-Authenticate": "Bearer"},
    )


@app.exception_handler(ForbiddenError)
async def forbidden_exception_handler(request, exc: ForbiddenError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(RestrictionViolationError)
async def restriction_violation_handler(request, exc: RestrictionViolationError):
    return JSONResponse(status_code=exc.status_code, content={"detail": exc.detail, "code": exc.code})


@app.exception_handler(PydanticValidationError)
async def pydantic_validation_exception_handler(request, exc: PydanticValidationError):
    errors = exc.errors()
    error_details = []
    for error in errors:
        loc = ".".join(map(str, error["loc"]))
        error_details.append(f"Field '{loc}': {error['msg']}")
    return JSONResponse(
        status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
        content={"detail": "Validation error: " + "; ".join(error_details), "code": "PYDANTIC_VALIDATION_ERROR"},
    )


# =============================================================================
# FUND ACCOUNTING ENDPOINTS (Concepts 1-15)
# =============================================================================


@app.post(
    "/funds/", response_model=models.FundInDB, status_code=status.HTTP_201_CREATED, dependencies=[Depends(lambda: None)]
)  # Add RBAC permission check
async def create_fund(
    fund: models.FundCreate, user_id: str = Depends(get_user_id), db_session: AsyncSession = Depends(lambda: None)
):
    """Create a new NPO fund"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_fund(session, user_id, fund)


@app.get("/funds/", response_model=List[models.FundInDB])
async def get_all_funds(user_id: str = Depends(get_user_id), fund_type: Optional[str] = Query(None)):
    """Get all funds, optionally filtered by type"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_all_funds(session, user_id, fund_type)


@app.get("/funds/{fund_id}", response_model=models.FundInDB)
async def get_fund(fund_id: str, user_id: str = Depends(get_user_id)):
    """Get fund by ID"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_fund(session, user_id, fund_id)


@app.post(
    "/funds/{fund_id}/transactions/", response_model=models.FundTransactionInDB, status_code=status.HTTP_201_CREATED
)
async def create_fund_transaction(
    fund_id: str, transaction: models.FundTransactionCreate, user_id: str = Depends(get_user_id)
):
    """Create fund transaction"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_fund_transaction(session, user_id, fund_id, transaction)


@app.get("/funds/{fund_id}/transactions/", response_model=List[models.FundTransactionInDB])
async def get_fund_transactions(
    fund_id: str,
    user_id: str = Depends(get_user_id),
    start_date: Optional[date] = None,
    end_date: Optional[date] = None,
):
    """Get transactions for a fund"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_fund_transactions(session, user_id, fund_id, start_date, end_date)


@app.post(
    "/funds/{fund_id}/restrictions/", response_model=models.FundRestrictionInDB, status_code=status.HTTP_201_CREATED
)
async def create_fund_restriction(
    fund_id: str, restriction: models.FundRestrictionCreate, user_id: str = Depends(get_user_id)
):
    """Create fund restriction"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_fund_restriction(session, user_id, fund_id, restriction)


# =============================================================================
# NET ASSETS ENDPOINTS (Concepts 16-25)
# =============================================================================


@app.post("/net-assets/", response_model=models.NetAssetsInDB, status_code=status.HTTP_201_CREATED)
async def create_net_assets(
    as_of_date: date, period_start: date, period_end: date, user_id: str = Depends(get_user_id)
):
    """Create net assets record"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_net_assets(session, user_id, as_of_date, period_start, period_end)


@app.get("/net-assets/{as_of_date}", response_model=models.NetAssetsInDB)
async def get_net_assets(as_of_date: date, user_id: str = Depends(get_user_id)):
    """Get net assets as of date"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_net_assets(session, user_id, as_of_date)


# =============================================================================
# REVENUE ENDPOINTS (Concepts 26-50)
# =============================================================================


@app.post("/donations/", response_model=models.DonationInDB, status_code=status.HTTP_201_CREATED)
async def create_donation(donation: models.DonationCreate, user_id: str = Depends(get_user_id)):
    """Create donation"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_donation(session, user_id, donation)


@app.get("/donations/", response_model=List[models.DonationInDB])
async def get_donations(user_id: str = Depends(get_user_id)):
    """Get all donations"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_donations(session, user_id)


@app.post("/grants/", response_model=models.GrantInDB, status_code=status.HTTP_201_CREATED)
async def create_grant(grant: models.GrantCreate, user_id: str = Depends(get_user_id)):
    """Create grant"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_grant(session, user_id, grant)


@app.get("/grants/", response_model=List[models.GrantInDB])
async def get_grants(user_id: str = Depends(get_user_id), status: Optional[models.GrantStatus] = None):
    """Get all grants, optionally filtered by status"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_grants(session, user_id, status)


@app.get("/grants/{grant_id}", response_model=models.GrantInDB)
async def get_grant(grant_id: str, user_id: str = Depends(get_user_id)):
    """Get grant by ID"""
    async with Neo4jConnector.get_driver().session() as session:
        grants = await crud.get_grants(session, user_id)
        for g in grants:
            if g.id == grant_id:
                return g
        raise NotFoundError(detail=f"Grant {grant_id} not found")


@app.post("/grants/{grant_id}/drawdowns/", status_code=status.HTTP_201_CREATED)
async def create_grant_drawdown(grant_id: str, drawdown: models.GrantDrawdownBase, user_id: str = Depends(get_user_id)):
    """Create grant drawdown"""
    # Implementation would update grant amount_received
    return {"status": "success", "grant_id": grant_id, "drawdown": drawdown}


# =============================================================================
# PROJECT AND PROGRAM ENDPOINTS
# =============================================================================


@app.post("/projects/", response_model=models.ProjectInDB, status_code=status.HTTP_201_CREATED)
async def create_project(project: models.ProjectCreate, user_id: str = Depends(get_user_id)):
    """Create NPO project"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_project(session, user_id, project)


@app.get("/projects/", response_model=List[models.ProjectInDB])
async def get_projects(user_id: str = Depends(get_user_id), status: Optional[models.ProjectStatus] = None):
    """Get all projects"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_projects(session, user_id, status)


@app.get("/projects/{project_id}", response_model=models.ProjectInDB)
async def get_project(project_id: str, user_id: str = Depends(get_user_id)):
    """Get project by ID"""
    async with Neo4jConnector.get_driver().session() as session:
        projects = await crud.get_projects(session, user_id)
        for p in projects:
            if p.id == project_id:
                return p
        raise NotFoundError(detail=f"Project {project_id} not found")


@app.post("/programs/", response_model=models.ProgramInDB, status_code=status.HTTP_201_CREATED)
async def create_program(program: models.ProgramCreate, user_id: str = Depends(get_user_id)):
    """Create NPO program"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_program(session, user_id, program)


@app.get("/programs/", response_model=List[models.ProgramInDB])
async def get_programs(user_id: str = Depends(get_user_id)):
    """Get all programs"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_programs(session, user_id)


# =============================================================================
# DONOR ENDPOINTS
# =============================================================================


@app.post("/donors/", response_model=models.DonorInDB, status_code=status.HTTP_201_CREATED)
async def create_donor(donor: models.DonorCreate, user_id: str = Depends(get_user_id)):
    """Create donor"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_donor(session, user_id, donor)


@app.get("/donors/", response_model=List[models.DonorInDB])
async def get_donors(user_id: str = Depends(get_user_id)):
    """Get all donors"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_donors(session, user_id)


@app.get("/donors/{donor_id}", response_model=models.DonorInDB)
async def get_donor(donor_id: str, user_id: str = Depends(get_user_id)):
    """Get donor by ID"""
    async with Neo4jConnector.get_driver().session() as session:
        donors = await crud.get_donors(session, user_id)
        for d in donors:
            if d.id == donor_id:
                return d
        raise NotFoundError(detail=f"Donor {donor_id} not found")


# =============================================================================
# BUDGET ENDPOINTS
# =============================================================================


@app.post("/budgets/", response_model=models.BudgetInDB, status_code=status.HTTP_201_CREATED)
async def create_budget(budget: models.BudgetCreate, user_id: str = Depends(get_user_id)):
    """Create budget"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_budget(session, user_id, budget)


@app.get("/budgets/", response_model=List[models.BudgetInDB])
async def get_budgets(user_id: str = Depends(get_user_id), fiscal_year: Optional[str] = None):
    """Get all budgets"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_budgets(session, user_id, fiscal_year)


@app.post("/budgets/{budget_id}/lines/", response_model=models.BudgetLineInDB, status_code=status.HTTP_201_CREATED)
async def create_budget_line(budget_id: str, line: models.BudgetLineCreate, user_id: str = Depends(get_user_id)):
    """Create budget line item"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_budget_line(session, user_id, budget_id, line)


# =============================================================================
# COMPLIANCE AND GOVERNANCE ENDPOINTS
# =============================================================================


@app.post("/internal-controls/", response_model=models.InternalControlInDB, status_code=status.HTTP_201_CREATED)
async def create_internal_control(control: models.InternalControlCreate, user_id: str = Depends(get_user_id)):
    """Create internal control"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_internal_control(session, user_id, control)


@app.get("/internal-controls/", response_model=List[models.InternalControlInDB])
async def get_internal_controls(user_id: str = Depends(get_user_id)):
    """Get all internal controls"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_internal_controls(session, user_id)


@app.post("/audit-reports/", response_model=models.AuditReportInDB, status_code=status.HTTP_201_CREATED)
async def create_audit_report(audit: models.AuditReportCreate, user_id: str = Depends(get_user_id)):
    """Create audit report"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_audit_report(session, user_id, audit)


@app.get("/audit-reports/", response_model=List[models.AuditReportInDB])
async def get_audit_reports(user_id: str = Depends(get_user_id)):
    """Get all audit reports"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_audit_reports(session, user_id)


# =============================================================================
# PERFORMANCE AND IMPACT ENDPOINTS
# =============================================================================


@app.post("/program-metrics/", status_code=status.HTTP_201_CREATED)
async def create_program_metric(metric: models.ProgramMetricCreate, user_id: str = Depends(get_user_id)):
    """Create program metric"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_program_metric(session, user_id, metric)


@app.post("/impact-measurements/", status_code=status.HTTP_201_CREATED)
async def create_impact_measurement(measurement: models.ImpactMeasurementCreate, user_id: str = Depends(get_user_id)):
    """Create impact measurement"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_impact_measurement(session, user_id, measurement)


@app.post("/sroi-analyses/", status_code=status.HTTP_201_CREATED)
async def create_sroi_analysis(analysis: models.SROIAnalysisBase, user_id: str = Depends(get_user_id)):
    """Create SROI analysis"""
    return {"status": "success", "analysis": analysis}


@app.post("/volunteer-records/", response_model=models.VolunteerRecordInDB, status_code=status.HTTP_201_CREATED)
async def create_volunteer_record(record: models.VolunteerRecordCreate, user_id: str = Depends(get_user_id)):
    """Create volunteer record"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_volunteer_record(session, user_id, record)


@app.get("/volunteer-records/", response_model=List[models.VolunteerRecordInDB])
async def get_volunteer_records(
    user_id: str = Depends(get_user_id), start_date: Optional[date] = None, end_date: Optional[date] = None
):
    """Get volunteer records"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_volunteer_records(session, user_id, start_date, end_date)


# =============================================================================
# FINANCIAL STATEMENT ENDPOINTS (Concepts 48-55)
# =============================================================================


@app.post(
    "/statements/activities/", response_model=models.StatementOfActivitiesInDB, status_code=status.HTTP_201_CREATED
)
async def create_statement_of_activities(period_start: date, period_end: date, user_id: str = Depends(get_user_id)):
    """Create Statement of Activities (equivalent to Income Statement)"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_statement_of_activities(session, user_id, period_start, period_end)


@app.get("/statements/activities/", response_model=Optional[models.StatementOfActivitiesInDB])
async def get_statement_of_activities(period_start: date, period_end: date, user_id: str = Depends(get_user_id)):
    """Get Statement of Activities for period"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.get_statement_of_activities(session, user_id, period_start, period_end)


@app.post(
    "/statements/financial-position/",
    response_model=models.StatementOfFinancialPositionInDB,
    status_code=status.HTTP_201_CREATED,
)
async def create_statement_of_financial_position(as_of_date: date, user_id: str = Depends(get_user_id)):
    """Create Statement of Financial Position (equivalent to Balance Sheet)"""
    async with Neo4jConnector.get_driver().session() as session:
        return await crud.create_statement_of_financial_position(session, user_id, as_of_date)


# =============================================================================
# NPO ASSET ENDPOINTS
# =============================================================================


@app.post("/assets/", response_model=models.NPOAssetInDB, status_code=status.HTTP_201_CREATED)
async def create_npo_asset(asset: models.NPOAssetCreate, user_id: str = Depends(get_user_id)):
    """Create NPO asset"""
    asset_id = str(uuid.uuid4())
    net_book_value = asset.acquisition_cost
    return models.NPOAssetInDB(
        id=asset_id,
        user_id=user_id,
        asset_name=asset.asset_name,
        asset_type=asset.asset_type,
        category=asset.category,
        acquisition_date=asset.acquisition_date,
        acquisition_cost=asset.acquisition_cost,
        current_value=asset.current_value,
        useful_life_years=asset.useful_life_years,
        depreciation_method=asset.depreciation_method,
        salvage_value=asset.salvage_value,
        location=asset.location,
        responsible_person=asset.responsible_person,
        status=asset.status,
        fund_id=asset.fund_id,
        asset_code=f"AST-{asset_id[:8]}",
        accumulated_depreciation=Decimal("0.00"),
        net_book_value=net_book_value,
        created_at=datetime.utcnow(),
    )


# =============================================================================
# HEALTH CHECK
# =============================================================================


@app.get("/health")
async def health_check():
    """Health check endpoint"""
    return {"status": "healthy", "service": "npo-service", "version": "0.1.0"}


import uuid
