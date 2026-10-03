"""Vimbai Accounting Standards Service. Port: 8095

Supports all major accounting standards worldwide (IFRS, US GAAP, UK GAAP,
EU Directives, Asian, Middle East, African, Australian, Canadian and more).

Organization configuration, account mappings, accounting policies and
compliance checks persist to Neo4j as caller-owned, Book-scoped records
(previously four module-level dicts shared across ALL callers). The
standards/requirements catalogues remain code-defined seeds.

This file may be imported bare (bracket mounts, uvicorn main:app), so it
bootstraps its own package alias before importing sibling modules.
"""

import importlib.util
import os as _os
import sys as _sys

_HERE = _os.path.dirname(_os.path.abspath(__file__))
if "accounting_standards_service" not in _sys.modules or not hasattr(
    _sys.modules.get("accounting_standards_service"), "__path__"
):
    _spec = importlib.util.spec_from_file_location("accounting_standards_service", _os.path.join(_HERE, "__init__.py"))
    _pkg = importlib.util.module_from_spec(_spec)
    _spec.loader.exec_module(_pkg)
    _sys.modules["accounting_standards_service"] = _pkg
    _sys.modules["accounting_standards_service"].__path__ = [_HERE]

import os
import uuid
from datetime import date, datetime, timezone
from typing import Any, Dict, List, Optional

import structlog
from accounting_standards_service import crud, models
from accounting_standards_service.dependencies import book_id_var, get_db_session, get_user_id
from accounting_standards_service.exceptions import AccountingStandardsError
from accounting_standards_service.models import (
    AccountCategory,
    AccountingPolicy,
    AccountingStandard,
    AccountMapping,
    ComplianceCheck,
    DisclosureLevel,
    MeasurementBase,
    StandardConfiguration,
    StandardRequirement,
    StandardType,
)
from fastapi import Depends, FastAPI, HTTPException, Request
from neo4j import AsyncSession

SERVICE_NAME = "accounting-standards-service"
PORT = int(os.getenv("PORT", "8095"))
structlog.configure(
    processors=[
        structlog.stdlib.add_log_level,
        structlog.processors.TimeStamper(fmt="iso"),
        structlog.processors.JSONRenderer(),
    ],
    wrapper_class=structlog.stdlib.BoundLogger,
    logger_factory=structlog.stdlib.LoggerFactory(),
    cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(
    title="Vimbai Accounting Standards Service",
    description="Comprehensive accounting standards management supporting IFRS, US GAAP, UK GAAP, and 40+ national standards",
    version="2.0.0",
)

# Distributed tracing (OpenTelemetry)
try:
    from shared.tracing import setup_tracing

    TRACER = setup_tracing(service_name=SERVICE_NAME, instrument_app=app)
except ImportError:
    TRACER = None


@app.middleware("http")
async def book_context_middleware(request: Request, call_next):
    """Propagate the Book context (X-Book-ID, verified upstream) to the CRUD layer."""
    book_id_var.set(request.headers.get("X-Book-ID"))
    return await call_next(request)


@app.exception_handler(AccountingStandardsError)
async def _accounting_standards_error(request: Request, exc: AccountingStandardsError):
    from fastapi.responses import JSONResponse

    status = getattr(exc, "status_code", 400)
    return JSONResponse(status_code=status, content={"detail": str(exc), "error": exc.__class__.__name__})


# ============================================================================
# Accounting Standards Database
# ============================================================================

ACCOUNTING_STANDARDS: Dict[str, AccountingStandard] = {
    "ifrs": AccountingStandard(
        id="ifrs-2024",
        code="IFRS",
        name="International Financial Reporting Standards",
        standard_type=StandardType.IFRS,
        region="International",
        country="Global",
        issuing_body="IFRS Foundation / IASB",
        effective_date=date(2024, 1, 1),
        version="2024",
        description="Global accounting standards issued by IASB for transparent and accountable financial reporting",
        key_principles=[
            "Fair value measurement emphasis",
            "Substance over form",
            "Single accounting model for all entities",
            "Principle-based approach",
            "IFRS for SMEs separate standard",
        ],
        measurement_basis=MeasurementBase.FAIR_VALUE,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["IFRS for SMEs", "IAS", "SIC"],
    ),
    "us_gaap": AccountingStandard(
        id="us_gaap_2024",
        code="US GAAP",
        name="US Generally Accepted Accounting Principles",
        standard_type=StandardType.US_GAAP,
        region="North America",
        country="United States",
        issuing_body="FASB",
        effective_date=date(2024, 1, 1),
        version="2024",
        description="Accounting rules and procedures for financial reporting in the United States",
        key_principles=[
            "Historical cost emphasis",
            "Revenue recognition (ASC 606)",
            "Matching principle",
            "Conservatism",
            "Industry-specific guidance",
        ],
        measurement_basis=MeasurementBase.HISTORICAL_COST,
        inflation_adjustment_required=False,
        consolidation_method="majority voting control",
        related_standards=["SEC", "FASB ASC", "EITF"],
    ),
    "uk_gaap": AccountingStandard(
        id="frs_102",
        code="FRS 102",
        name="Financial Reporting Standard 102",
        standard_type=StandardType.UK_GAAP,
        region="Europe",
        country="United Kingdom",
        issuing_body="FRC",
        effective_date=date(2015, 1, 1),
        version="2024",
        description="UK accounting standard for small and medium entities, based on IFRS principles",
        key_principles=[
            "IFRS principles adapted for UK",
            "Historical cost with some fair value",
            "Simplified revenue recognition",
            "Straightforward presentation",
            "Three-tier accounting model",
        ],
        measurement_basis=MeasurementBase.MIXED,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["FRS 101", "FRS 105", "Companies Act 2006"],
    ),
    "indian_gaap": AccountingStandard(
        id="ind_as",
        code="Ind AS",
        name="Indian Accounting Standards",
        standard_type=StandardType.INDIAN_GAAP,
        region="Asia",
        country="India",
        issuing_body="MCA / ICAI",
        effective_date=date(2016, 4, 1),
        version="2024",
        description="Indian accounting standards converged with IFRS for listed and large entities",
        key_principles=[
            "IFRS convergence",
            "Schedule III presentation",
            "Indian tax law integration",
            "Transfer pricing requirements",
            "Foreign currency accounting",
        ],
        measurement_basis=MeasurementBase.FAIR_VALUE,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["Companies Act 2013", "SEBI", "Income Tax Act"],
    ),
    "japanese_gaap": AccountingStandard(
        id="j_gaap",
        code="J-GAAP",
        name="Japanese Generally Accepted Accounting Principles",
        standard_type=StandardType.JAPANESE_GAAP,
        region="Asia",
        country="Japan",
        issuing_body="ASBJ",
        effective_date=date(2024, 1, 1),
        version="2024",
        description="Japanese accounting standards with unique practices like tax effect accounting",
        key_principles=[
            "Tax effect accounting",
            "Historical cost basis",
            "Group accounting rules",
            "Retained earnings appropriation",
            "Un西山disclosed reserves",
        ],
        measurement_basis=MeasurementBase.HISTORICAL_COST,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["J-IFRS", "J-SOX", "Tax Code"],
    ),
    "chinese_gaap": AccountingStandard(
        id="cas",
        code="CAS",
        name="Chinese Accounting Standards",
        standard_type=StandardType.CHINESE_GAAP,
        region="Asia",
        country="China",
        issuing_body="Ministry of Finance",
        effective_date=date(2007, 1, 1),
        version="2024",
        description="Chinese accounting standards for business enterprises with PRC characteristics",
        key_principles=[
            "Historical cost with fair value option",
            "Government subsidies treatment",
            "Related party disclosure emphasis",
            "Statutory reserve requirements",
            "RMB as functional currency",
        ],
        measurement_basis=MeasurementBase.MIXED,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["PRC Company Law", "Securities Law", "Tax Law"],
    ),
    "australian_gaap": AccountingStandard(
        id="aifrs",
        code="AIFRS",
        name="Australian International Financial Reporting Standards",
        standard_type=StandardType.AUSTRALIAN_GAAP,
        region="Oceania",
        country="Australia",
        issuing_body="AASB",
        effective_date=date(2005, 1, 1),
        version="2024",
        description="Australian accounting standards aligned with IFRS",
        key_principles=[
            "IFRS adoption",
            "Urgent Issues Group opinions",
            "Tax effect accounting (prior to 2022)",
            "Superannuation accounting",
            "Tax consolidated groups",
        ],
        measurement_basis=MeasurementBase.FAIR_VALUE,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["Corporations Act 2001", "ASIC", "Superannuation Law"],
    ),
    "canadian_aspe": AccountingStandard(
        id="aspe",
        code="ASPE",
        name="Accounting Standards for Private Enterprises",
        standard_type=StandardType.CANADIAN_ASPE,
        region="North America",
        country="Canada",
        issuing_body="AcSB",
        effective_date=date(2011, 1, 1),
        version="2024",
        description="Canadian accounting standards for private enterprises, alternative to IFRS",
        key_principles=[
            "Private entity focus",
            "Cost-based measurements",
            "Simplified presentation",
            "Income tax allocation",
            "Related party disclosures",
        ],
        measurement_basis=MeasurementBase.HISTORICAL_COST,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["CPA Canada Handbook - Part II", "ASPE"],
    ),
    "german_gaap": AccountingStandard(
        id="hgb",
        code="HGB",
        name="German Commercial Code Accounting",
        standard_type=StandardType.GERMAN_GAAP,
        region="Europe",
        country="Germany",
        issuing_body="Federal Ministry of Justice",
        effective_date=date(2024, 1, 1),
        version="2024",
        description="German accounting under Commercial Code (HGB) with tax-driven principles",
        key_principles=[
            "Imperative valuation (BiB)",
            "Principle of prudence (Vorsichtsprinzip)",
            "Lower of cost or market",
            "Hidden reserves allowed",
            "Tax balance sheet linkage",
        ],
        measurement_basis=MeasurementBase.HISTORICAL_COST,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["AktG", "GmbHG", "EstG", "UStG"],
    ),
    "uae_gaap": AccountingStandard(
        id="uae_gaap",
        code="UAE GAAP",
        name="UAE Accounting Standards",
        standard_type=StandardType.UAE_GAAP,
        region="Middle East",
        country="United Arab Emirates",
        issuing_body="ESMA / Ministry of Economy",
        effective_date=date(2022, 1, 1),
        version="2024",
        description="UAE accounting standards for entities in the UAE including ADGM and DIFC frameworks",
        key_principles=[
            "IFRS adoption for listed entities",
            "UAE Federal Law requirements",
            "Sharia compliance for Islamic finance",
            "VAT accounting requirements",
            "Free zone special considerations",
        ],
        measurement_basis=MeasurementBase.FAIR_VALUE,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["Federal Law No. 2 of 2015", "ADGM", "DIFC"],
    ),
    "saudi_gaap": AccountingStandard(
        id="sasr",
        code="SASR",
        name="Saudi Arabian Accounting Standards",
        standard_type=StandardType.SAUDI_GAAP,
        region="Middle East",
        country="Saudi Arabia",
        issuing_body="SOCPA",
        effective_date=date(2023, 1, 1),
        version="2024",
        description="Saudi Arabian accounting standards aligned with IFRS for listed companies",
        key_principles=[
            "IFRS-based for listed entities",
            "Zakat and tax accounting",
            "Sharia-compliant transactions",
            "SAMA regulations",
            "Capital market requirements",
        ],
        measurement_basis=MeasurementBase.FAIR_VALUE,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["Zakat Regulations", "SAMA", "Tadawul"],
    ),
    "singapore_gaap": AccountingStandard(
        id="sfrs",
        code="SFRS",
        name="Singapore Financial Reporting Standards",
        standard_type=StandardType.SINGAPORE_GAAP,
        region="Asia",
        country="Singapore",
        issuing_body="ACRA / IASB",
        effective_date=date(2003, 1, 1),
        version="2024",
        description="Singapore accounting standards aligned with IFRS for Singapore entities",
        key_principles=[
            "IFRS adoption",
            "Singapore-specific disclosures",
            "Statutory reserves",
            "Related party regulations",
            "Tax transparency",
        ],
        measurement_basis=MeasurementBase.FAIR_VALUE,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["Companies Act", "ACRA", "IRAS"],
    ),
    "hk_gaap": AccountingStandard(
        id="hkfrs",
        code="HKFRS",
        name="Hong Kong Financial Reporting Standards",
        standard_type=StandardType.HK_GAAP,
        region="Asia",
        country="Hong Kong",
        issuing_body="HKICPA",
        effective_date=date(2005, 1, 1),
        version="2024",
        description="Hong Kong accounting standards aligned with IFRS",
        key_principles=[
            "IFRS convergence",
            "Small entity exemptions",
            "Property valuation",
            "Related party disclosures",
            "HKSE listing requirements",
        ],
        measurement_basis=MeasurementBase.FAIR_VALUE,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["Companies Ordinance", "HKSE", "HKICPA"],
    ),
    "south_african_gaap": AccountingStandard(
        id="ifrs_grap",
        code="GRAP",
        name="South African Standards of GRAP",
        standard_type=StandardType.SOUTH_AFRICAN_GAAP,
        region="Africa",
        country="South Africa",
        issuing_body="ASB",
        effective_date=date(2014, 4, 1),
        version="2024",
        description="South African accounting standards for government and public entities",
        key_principles=[
            "GRAP standards for government",
            "IFRS for listed entities",
            "PFMA compliance",
            "Treasury regulations",
            "Public sector accounting",
        ],
        measurement_basis=MeasurementBase.FAIR_VALUE,
        inflation_adjustment_required=False,
        consolidation_method="control",
        related_standards=["PFMA", "MFMA", "Treasury", "Companies Act"],
    ),
}


# ============================================================================
# Standard Requirements Database
# ============================================================================

STANDARD_REQUIREMENTS: Dict[str, List[StandardRequirement]] = {
    "ifrs": [
        StandardRequirement(
            standard=StandardType.IFRS,
            requirement_code="IFRS_15",
            description="Revenue from contracts with customers",
            category="Revenue",
            is_mandatory=True,
            effective_date=date(2018, 1, 1),
            disclosure_required=True,
            measurement_method="5-step model",
        ),
        StandardRequirement(
            standard=StandardType.IFRS,
            requirement_code="IFRS_16",
            description="Lease accounting",
            category="Leases",
            is_mandatory=True,
            effective_date=date(2019, 1, 1),
            disclosure_required=True,
            measurement_method="right-of-use asset",
        ),
        StandardRequirement(
            standard=StandardType.IFRS,
            requirement_code="IFRS_9",
            description="Financial instruments classification and measurement",
            category="Financial Instruments",
            is_mandatory=True,
            effective_date=date(2018, 1, 1),
            disclosure_required=True,
            measurement_method="fair value through P&L or OCI",
        ),
        StandardRequirement(
            standard=StandardType.IFRS,
            requirement_code="IFRS_13",
            description="Fair value measurement",
            category="Measurement",
            is_mandatory=True,
            effective_date=date(2013, 1, 1),
            disclosure_required=True,
            measurement_method="market approach, income approach, cost approach",
        ),
    ],
    "us_gaap": [
        StandardRequirement(
            standard=StandardType.US_GAAP,
            requirement_code="ASC_606",
            description="Revenue from contracts with customers",
            category="Revenue",
            is_mandatory=True,
            effective_date=date(2018, 1, 1),
            disclosure_required=True,
            measurement_method="5-step model",
        ),
        StandardRequirement(
            standard=StandardType.US_GAAP,
            requirement_code="ASC_842",
            description="Lease accounting",
            category="Leases",
            is_mandatory=True,
            effective_date=date(2019, 1, 1),
            disclosure_required=True,
            measurement_method="right-of-use asset",
        ),
    ],
}

# ============================================================================
# API Endpoints
# ============================================================================


@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "accounting-standards",
        "version": "2.0.0",
        "supported_standards": len(ACCOUNTING_STANDARDS),
    }


@app.get("/standards")
async def list_standards(region: Optional[str] = None, standard_type: Optional[StandardType] = None):
    """List all available accounting standards"""
    result = list(ACCOUNTING_STANDARDS.values())

    if region:
        result = [s for s in result if s.region.lower() == region.lower()]
    if standard_type:
        result = [s for s in result if s.standard_type == standard_type]

    return result


@app.get("/standards/comparison")
async def compare_standards(standard_1: StandardType, standard_2: StandardType):
    """Compare two accounting standards"""
    s1 = await get_standard(standard_1)
    s2 = await get_standard(standard_2)

    return {
        "standard_1": s1.model_dump(),
        "standard_2": s2.model_dump(),
        "comparison": {
            "measurement_basis_differences": s1.measurement_basis != s2.measurement_basis,
            "consolidation_method_differences": s1.consolidation_method != s2.consolidation_method,
            "key_principles_common": [p for p in s1.key_principles if p in s2.key_principles],
            "unique_to_standard_1": [p for p in s1.key_principles if p not in s2.key_principles],
            "unique_to_standard_2": [p for p in s2.key_principles if p not in s1.key_principles],
        },
    }


@app.get("/standards/categories")
async def list_standard_categories():
    """List all standard categories by region"""
    categories = {}
    for standard in ACCOUNTING_STANDARDS.values():
        if standard.region not in categories:
            categories[standard.region] = []
        categories[standard.region].append(
            {
                "code": standard.standard_type.value,
                "name": standard.name,
                "country": standard.country,
            }
        )
    return categories


@app.get("/standards/{standard_type}")
async def get_standard(standard_type: StandardType):
    """Get details of a specific accounting standard"""
    # Find by standard_type value
    for key, standard in ACCOUNTING_STANDARDS.items():
        if standard.standard_type == standard_type:
            return standard

    # Also try by key
    if standard_type.value in ACCOUNTING_STANDARDS:
        return ACCOUNTING_STANDARDS[standard_type.value]

    raise HTTPException(status_code=404, detail="Standard not found")


@app.get("/standards/{standard_type}/requirements")
async def get_standard_requirements(standard_type: StandardType):
    """Get requirements for a specific standard"""
    key = standard_type.value
    if key in STANDARD_REQUIREMENTS:
        return STANDARD_REQUIREMENTS[key]
    return []


# --- Organization Configuration (caller-owned, Book-scoped) ---


@app.post("/organizations/{organization_id}/configuration")
async def create_standard_configuration(
    organization_id: str,
    config: StandardConfiguration,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Configure accounting standards for an organization (upserts the caller's config)."""
    config.organization_id = organization_id
    config.created_at = datetime.now(timezone.utc)
    config.updated_at = datetime.now(timezone.utc)

    return await crud.replace_for(db_session, user_id, config, organization_id=organization_id)


@app.get("/organizations/{organization_id}/configuration")
async def get_standard_configuration(
    organization_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's standard configuration for an organization; cross-scope 404."""
    configs = [
        c
        for c in await crud.list_all(db_session, user_id, StandardConfiguration)
        if c.organization_id == organization_id
    ]
    if not configs:
        raise HTTPException(status_code=404, detail="Configuration not found")
    return configs[0]


@app.put("/organizations/{organization_id}/configuration")
async def update_standard_configuration(
    organization_id: str,
    config: StandardConfiguration,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Update the caller's standard configuration; cross-scope 404."""
    existing = [
        c
        for c in await crud.list_all(db_session, user_id, StandardConfiguration)
        if c.organization_id == organization_id
    ]
    if not existing:
        raise HTTPException(status_code=404, detail="Configuration not found")

    config.id = existing[0].id
    config.organization_id = organization_id
    config.created_at = existing[0].created_at
    config.updated_at = datetime.now(timezone.utc)

    await crud.update(db_session, user_id, config)
    return config


@app.post("/organizations/{organization_id}/standards/{standard_type}/activate")
async def activate_standard(
    organization_id: str,
    standard_type: StandardType,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add a standard to the organization's active standards; cross-scope 404."""
    configs = [
        c
        for c in await crud.list_all(db_session, user_id, StandardConfiguration)
        if c.organization_id == organization_id
    ]
    if not configs:
        raise HTTPException(status_code=404, detail="Organization configuration not found")

    config = configs[0]
    if standard_type not in config.selected_standards:
        config.selected_standards.append(standard_type)
        config.updated_at = datetime.now(timezone.utc)
        await crud.update(db_session, user_id, config)

    return {"status": "activated", "standard": standard_type.value}


@app.post("/organizations/{organization_id}/standards/{standard_type}/deactivate")
async def deactivate_standard(
    organization_id: str,
    standard_type: StandardType,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Remove a standard from the organization's active standards; cross-scope 404."""
    configs = [
        c
        for c in await crud.list_all(db_session, user_id, StandardConfiguration)
        if c.organization_id == organization_id
    ]
    if not configs:
        raise HTTPException(status_code=404, detail="Organization configuration not found")

    config = configs[0]
    if standard_type in config.selected_standards:
        config.selected_standards.remove(standard_type)
        config.updated_at = datetime.now(timezone.utc)
        await crud.update(db_session, user_id, config)

    return {"status": "deactivated", "standard": standard_type.value}


# --- Account Mapping (caller-owned, Book-scoped) ---


@app.get("/standards/{standard_type}/account-mapping")
async def get_account_mapping(
    standard_type: StandardType,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's chart of accounts mapping for a standard."""
    return [m for m in await crud.list_all(db_session, user_id, AccountMapping) if m.standard_type == standard_type]


@app.post("/standards/{standard_type}/account-mapping")
async def add_account_mapping(
    standard_type: StandardType,
    mapping: AccountMapping,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Add an account mapping for a standard (keyed by the path standard)."""
    mapping.standard_type = standard_type
    await crud.create(db_session, user_id, mapping)
    return {"status": "added", "mapping": mapping}


# --- Accounting Policies (caller-owned, Book-scoped) ---


@app.post("/organizations/{organization_id}/policies")
async def create_accounting_policy(
    organization_id: str,
    policy: AccountingPolicy,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Create an accounting policy (upserts the caller's policy for org+standard+area)."""
    policy.id = str(uuid.uuid4())
    policy.organization_id = organization_id
    policy.created_at = datetime.now(timezone.utc)

    return await crud.replace_for(
        db_session,
        user_id,
        policy,
        organization_id=organization_id,
        standard_type=policy.standard_type.value,
        policy_area=policy.policy_area,
    )


@app.get("/organizations/{organization_id}/policies")
async def list_accounting_policies(
    organization_id: str,
    standard_type: Optional[StandardType] = None,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """List the caller's accounting policies for an organization."""
    policies = [
        p for p in await crud.list_all(db_session, user_id, AccountingPolicy) if p.organization_id == organization_id
    ]

    if standard_type:
        policies = [p for p in policies if p.standard_type == standard_type]

    return policies


@app.get("/organizations/{organization_id}/policies/{policy_area}")
async def get_policy_for_area(
    organization_id: str,
    policy_area: str,
    standard_type: StandardType,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's policy for a specific area and standard; cross-scope 404."""
    for p in await crud.list_all(db_session, user_id, AccountingPolicy):
        if p.organization_id == organization_id and p.policy_area == policy_area and p.standard_type == standard_type:
            return p
    raise HTTPException(status_code=404, detail="Policy not found")


# --- Compliance Checking (caller-owned, Book-scoped) ---


@app.post("/organizations/{organization_id}/compliance/check")
async def run_compliance_check(
    organization_id: str,
    standard_type: StandardType,
    check_data: Dict[str, Any],
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Run a compliance check against standard requirements; replaces the caller's history for the org."""
    checks = []
    requirements = STANDARD_REQUIREMENTS.get(standard_type.value, [])

    for req in requirements:
        check = ComplianceCheck(
            id=str(uuid.uuid4()),
            standard_type=standard_type,
            check_date=datetime.now(timezone.utc),
            status="pass",
            area=req.category,
            requirement=req.requirement_code,
        )

        # Simulate validation based on check_data
        if "validation_results" in check_data:
            for result in check_data["validation_results"]:
                if result.get("code") == req.requirement_code:
                    check.status = result.get("status", "pass")
                    check.finding = result.get("finding")
                    check.severity = result.get("severity")
                    check.recommendation = result.get("recommendation")

        checks.append(check)

    # Replace the caller's compliance history for this organization
    await crud.delete_where(db_session, user_id, ComplianceCheck, organization_id=organization_id)
    for check in checks:
        await crud.create(db_session, user_id, check, extra={"organization_id": organization_id})

    return {
        "organization_id": organization_id,
        "standard": standard_type.value,
        "total_checks": len(checks),
        "passed": sum(1 for c in checks if c.status == "pass"),
        "failed": sum(1 for c in checks if c.status == "fail"),
        "warnings": sum(1 for c in checks if c.status == "warning"),
        "checks": checks,
    }


@app.get("/organizations/{organization_id}/compliance/history")
async def get_compliance_history(
    organization_id: str,
    user_id: str = Depends(get_user_id),
    db_session: AsyncSession = Depends(get_db_session),
):
    """Get the caller's compliance check history for an organization."""
    return await crud.list_where(db_session, user_id, ComplianceCheck, organization_id=organization_id)


@app.get("/standards/{standard_type}/disclosure-requirements")
async def get_disclosure_requirements(standard_type: StandardType):
    """Get disclosure requirements for a standard (code-defined catalogue)."""
    requirements = STANDARD_REQUIREMENTS.get(standard_type.value, [])
    disclosures = [
        {
            "code": req.requirement_code,
            "description": req.description,
            "category": req.category,
            "is_mandatory": req.is_mandatory,
            "disclosure_required": req.disclosure_required,
        }
        for req in requirements
        if req.disclosure_required
    ]
    return disclosures


@app.get("/standards/{standard_type}/measurement-guide")
async def get_measurement_guide(standard_type: StandardType):
    """Get measurement guidance for a standard"""
    standard = await get_standard(standard_type)

    guides = {
        "ifrs": {
            "fair_value": "Use market participant assumptions, prioritize observable inputs",
            "historical_cost": "Rarely used except for some assets under IFRS 9",
            "present_value": "Discount using market rate for similar instruments",
        },
        "us_gaap": {
            "historical_cost": "Primary measurement basis, fair value option available",
            "fair_value": "Used for financial instruments, asset impairments, business combinations",
            "present_value": "Used for leases, asset retirement obligations, environmental liabilities",
        },
    }

    base_guide = guides.get(
        standard_type.value,
        {
            "default": f"Measurement basis: {standard.measurement_basis.value}",
        },
    )

    return {
        "standard": standard_type.value,
        "measurement_basis": standard.measurement_basis.value,
        "guidance": base_guide,
    }


if __name__ == "__main__":
    import uvicorn

    uvicorn.run(app, host="0.0.0.0", port=PORT)
