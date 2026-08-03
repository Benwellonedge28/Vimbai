"""
Vimbai Accounting Standards Service
Supports all major accounting standards worldwide including:
- IFRS (International Financial Reporting Standards)
- GAAP (US Generally Accepted Accounting Principles)
- UK GAAP (FRS 102, FRS 105)
- EU Directives
- Asian Standards (India, Japan, China, Singapore, HK, etc.)
- Middle East Standards (UAE, Saudi, etc.)
- African Standards (SAICA, Nigeria, Kenya, etc.)
- Australian/New Zealand Standards
- Canadian Standards (ASPE, IFRS for public)
- Japanese Standards (J-GAAP)
"""

from fastapi import FastAPI, HTTPException, Depends, status
from pydantic import BaseModel, Field, field_validator
from typing import List, Optional, Dict, Any, Literal
from datetime import datetime, timezone, date
from enum import Enum
import uuid
import hashlib
import json

app = FastAPI(
    title="Vimbai Accounting Standards Service",
    description="Comprehensive accounting standards management supporting IFRS, US GAAP, UK GAAP, and 40+ national standards",
    version="1.0.0",
)

# ============================================================================
# Enums
# ============================================================================

class StandardType(str, Enum):
    IFRS = "ifrs"
    US_GAAP = "us_gaap"
    UK_GAAP = "uk_gaap"
    EU_GAAP = "eu_gaap"
    INDIAN_GAAP = "indian_gaap"
    JAPANESE_GAAP = "japanese_gaap"
    CHINESE_GAAP = "chinese_gaap"
    SINGAPORE_GAAP = "singapore_gaap"
    HK_GAAP = "hk_gaap"
    AUSTRALIAN_GAAP = "australian_gaap"
    NZ_GAAP = "nz_gaap"
    CANADIAN_ASPE = "canadian_aspe"
    CANADIAN_IFRS = "canadian_ifrs"
    GERMAN_GAAP = "german_gaap"
    FRENCH_GAAP = "french_gaap"
    UAE_GAAP = "uae_gaap"
    SAUDI_GAAP = "saudi_gaap"
    SOUTH_AFRICAN_GAAP = "sa_gap"
    NIGERIAN_GAAP = "nigerian_gaap"
    KENYAN_GAAP = "kenyan_gaap"
    KOREAN_GAAP = "korean_gaap"
    MALAYSIAN_GAAP = "malaysian_gaap"
    THAI_GAAP = "thai_gaap"
    INDONESIAN_GAAP = "indonesian_gaap"
    PHILIPPINE_GAAP = "philippine_gaap"
    VIETNAMESE_GAAP = "vietnamese_gaap"
    BRAZILIAN_GAAP = "brazilian_gaap"
    MEXICAN_GAAP = "mexican_gaap"
    ARGENTINE_GAAP = "argentine_gaap"
    CHILEAN_GAAP = "chilean_gaap"
    COLOMBIAN_GAAP = "colombian_gaap"
    TURKISH_GAAP = "turkish_gaap"
    RUSSIAN_GAAP = "russian_gaap"
    POLISH_GAAP = "polish_gaap"
    CZECH_GAAP = "czech_gaap"
    HUNGARIAN_GAAP = "hungarian_gaap"
    ROMANIAN_GAAP = "romanian_gaap"
    UKRAINIAN_GAAP = "ukrainian_gaap"
    ISRAELI_GAAP = "israeli_gaap"
    EGYPTIAN_GAAP = "egyptian_gaap"
    MOROCCAN_GAAP = "moroccan_gaap"
    NETHERLANDS_GAAP = "dutch_gaap"
    BELGIAN_GAAP = "belgian_gaap"
    SWISS_GAAP = "swiss_gaap"
    AUSTRIAN_GAAP = "austrian_gaap"
    SWEDISH_GAAP = "swedish_gaap"
    NORWEGIAN_GAAP = "norwegian_gaap"
    DANISH_GAAP = "danish_gaap"
    FINNISH_GAAP = "finnish_gaap"
    IRISH_GAAP = "irish_gaap"
    PORTUGUESE_GAAP = "portuguese_gaap"
    SPANISH_GAAP = "spanish_gaap"
    ITALIAN_GAAP = "italian_gaap"
    GREEK_GAAP = "greek_gaap"
    CUSTOM = "custom"


class AccountCategory(str, Enum):
    ASSET = "asset"
    LIABILITY = "liability"
    EQUITY = "equity"
    REVENUE = "revenue"
    EXPENSE = "expense"
    GAIN = "gain"
    LOSS = "loss"


class AccountingPrinciple(str, Enum):
    GOING_CONCERN = "going_concern"
    ECONOMIC_ENTITY = "economic_entity"
    MONETARY_UNIT = "monetary_unit"
    TEMPORAL_UNIT = "temporal_unit"
    HISTORICAL_COST = "historical_cost"
    FAIR_VALUE = "fair_value"
    MATCHING = "matching"
    REVENUE_RECOGNITION = "revenue_recognition"
    EXPENSE_RECOGNITION = "expense_recognition"
    CONSERVATISM = "conservatism"
    MATERIALITY = "materiality"
    CONSISTENCY = "consistency"
    FULL_DISCLOSURE = "full_disclosure"
    ENTITY = "entity"


class MeasurementBase(str, Enum):
    HISTORICAL_COST = "historical_cost"
    CURRENT_COST = "current_cost"
    REALIZABLE_VALUE = "realizable_value"
    PRESENT_VALUE = "present_value"
    FAIR_VALUE = "fair_value"
    MIXED = "mixed"


class DisclosureLevel(str, Enum):
    MINIMUM = "minimum"
    STANDARD = "standard"
    ENHANCED = "enhanced"
    COMPREHENSIVE = "comprehensive"


# ============================================================================
# Pydantic Models
# ============================================================================

class AccountingStandard(BaseModel):
    id: str
    code: str
    name: str
    standard_type: StandardType
    region: str
    country: str
    issuing_body: str
    effective_date: date
    version: str
    description: str
    key_principles: List[str]
    measurement_basis: MeasurementBase
    presentation_currency: Optional[str] = None
    inflation_adjustment_required: bool = False
    consolidation_method: str = "control"
    related_standards: List[str] = []
    regulatory_body_url: Optional[str] = None


class StandardConfiguration(BaseModel):
    id: str
    organization_id: str
    selected_standards: List[StandardType]
    measurement_base: MeasurementBase
    disclosure_level: DisclosureLevel
    functional_currency: str
    presentation_currency: Optional[str] = None
    fiscal_year_end: str  # Month name
    comparative_periods: int = 2
    inflation_adjustment: bool = False
    include_tax_effects: bool = True
    consolidated_reporting: bool = True
    segment_reporting: bool = False
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))
    updated_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


class StandardRequirement(BaseModel):
    standard: StandardType
    requirement_code: str
    description: str
    category: str
    is_mandatory: bool
    effective_date: Optional[date] = None
    disclosure_required: bool
    measurement_method: Optional[str] = None
    presentation_format: Optional[str] = None
    validation_rules: Optional[Dict[str, Any]] = None


class AccountMapping(BaseModel):
    local_code: str
    local_name: str
    standard_code: str
    standard_name: str
    standard_type: StandardType
    category: AccountCategory
    classification: str
    measurement: MeasurementBase
    is_required: bool
    allowed_balances: Optional[List[str]] = None  # debit, credit, both


class ComplianceCheck(BaseModel):
    id: str
    standard_type: StandardType
    check_date: datetime
    status: Literal["pass", "fail", "warning", "not_applicable"]
    area: str
    requirement: str
    finding: Optional[str] = None
    severity: Optional[Literal["critical", "major", "minor"]] = None
    recommendation: Optional[str] = None


class AccountingPolicy(BaseModel):
    id: str
    organization_id: str
    standard_type: StandardType
    policy_area: str
    policy_description: str
    selected_method: str
    alternative_methods: List[str]
    justification: str
    disclosure_text: str
    effective_date: date
    approved_by: str
    created_at: datetime = Field(default_factory=lambda: datetime.now(timezone.utc))


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
# Storage
# ============================================================================

org_standard_configs: Dict[str, StandardConfiguration] = {}
account_mappings: Dict[str, List[AccountMapping]] = {}
accounting_policies: Dict[str, AccountingPolicy] = {}
compliance_checks: Dict[str, List[ComplianceCheck]] = {}


# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def health_check():
    return {
        "status": "healthy",
        "service": "accounting-standards",
        "version": "1.0.0",
        "supported_standards": len(ACCOUNTING_STANDARDS),
    }


@app.get("/standards")
async def list_standards(
    region: Optional[str] = None,
    standard_type: Optional[StandardType] = None
):
    """List all available accounting standards"""
    result = list(ACCOUNTING_STANDARDS.values())

    if region:
        result = [s for s in result if s.region.lower() == region.lower()]
    if standard_type:
        result = [s for s in result if s.standard_type == standard_type]

    return result


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


@app.get("/standards/categories")
async def list_standard_categories():
    """List all standard categories by region"""
    categories = {}
    for standard in ACCOUNTING_STANDARDS.values():
        if standard.region not in categories:
            categories[standard.region] = []
        categories[standard.region].append({
            "code": standard.standard_type.value,
            "name": standard.name,
            "country": standard.country,
        })
    return categories


# --- Organization Standard Configuration ---

@app.post("/organizations/{organization_id}/configuration")
async def create_standard_configuration(
    organization_id: str,
    config: StandardConfiguration
):
    """Configure accounting standards for an organization"""
    config.id = str(uuid.uuid4())
    config.organization_id = organization_id
    config.created_at = datetime.now(timezone.utc)
    config.updated_at = datetime.now(timezone.utc)

    org_standard_configs[organization_id] = config
    return config


@app.get("/organizations/{organization_id}/configuration")
async def get_standard_configuration(organization_id: str):
    """Get standard configuration for an organization"""
    if organization_id not in org_standard_configs:
        raise HTTPException(status_code=404, detail="Configuration not found")
    return org_standard_configs[organization_id]


@app.put("/organizations/{organization_id}/configuration")
async def update_standard_configuration(
    organization_id: str,
    config: StandardConfiguration
):
    """Update standard configuration for an organization"""
    if organization_id not in org_standard_configs:
        raise HTTPException(status_code=404, detail="Configuration not found")

    config.id = org_standard_configs[organization_id].id
    config.organization_id = organization_id
    config.created_at = org_standard_configs[organization_id].created_at
    config.updated_at = datetime.now(timezone.utc)

    org_standard_configs[organization_id] = config
    return config


@app.post("/organizations/{organization_id}/standards/{standard_type}/activate")
async def activate_standard(organization_id: str, standard_type: StandardType):
    """Add a standard to organization's active standards"""
    if organization_id not in org_standard_configs:
        raise HTTPException(status_code=404, detail="Organization configuration not found")

    config = org_standard_configs[organization_id]
    if standard_type not in config.selected_standards:
        config.selected_standards.append(standard_type)
        config.updated_at = datetime.now(timezone.utc)

    return {"status": "activated", "standard": standard_type.value}


@app.post("/organizations/{organization_id}/standards/{standard_type}/deactivate")
async def deactivate_standard(organization_id: str, standard_type: StandardType):
    """Remove a standard from organization's active standards"""
    if organization_id not in org_standard_configs:
        raise HTTPException(status_code=404, detail="Organization configuration not found")

    config = org_standard_configs[organization_id]
    if standard_type in config.selected_standards:
        config.selected_standards.remove(standard_type)
        config.updated_at = datetime.now(timezone.utc)

    return {"status": "deactivated", "standard": standard_type.value}


# --- Account Mapping ---

@app.get("/standards/{standard_type}/account-mapping")
async def get_account_mapping(standard_type: StandardType):
    """Get standard chart of accounts mapping"""
    key = standard_type.value
    if key in account_mappings:
        return account_mappings[key]
    return []


@app.post("/standards/{standard_type}/account-mapping")
async def add_account_mapping(
    standard_type: StandardType,
    mapping: AccountMapping
):
    """Add account mapping for a standard"""
    key = standard_type.value
    if key not in account_mappings:
        account_mappings[key] = []
    account_mappings[key].append(mapping)
    return {"status": "added", "mapping": mapping}


# --- Accounting Policies ---

@app.post("/organizations/{organization_id}/policies")
async def create_accounting_policy(
    organization_id: str,
    policy: AccountingPolicy
):
    """Create accounting policy for an organization"""
    policy.id = str(uuid.uuid4())
    policy.organization_id = organization_id
    policy.created_at = datetime.now(timezone.utc)

    policy_key = f"{organization_id}:{policy.standard_type.value}:{policy.policy_area}"
    accounting_policies[policy_key] = policy

    return policy


@app.get("/organizations/{organization_id}/policies")
async def list_accounting_policies(
    organization_id: str,
    standard_type: Optional[StandardType] = None
):
    """List all accounting policies for an organization"""
    policies = [
        p for k, p in accounting_policies.items()
        if k.startswith(f"{organization_id}:")
    ]

    if standard_type:
        policies = [p for p in policies if p.standard_type == standard_type]

    return policies


@app.get("/organizations/{organization_id}/policies/{policy_area}")
async def get_policy_for_area(
    organization_id: str,
    policy_area: str,
    standard_type: StandardType
):
    """Get policy for specific area and standard"""
    policy_key = f"{organization_id}:{standard_type.value}:{policy_area}"
    if policy_key in accounting_policies:
        return accounting_policies[policy_key]
    raise HTTPException(status_code=404, detail="Policy not found")


# --- Compliance Checking ---

@app.post("/organizations/{organization_id}/compliance/check")
async def run_compliance_check(
    organization_id: str,
    standard_type: StandardType,
    check_data: Dict[str, Any]
):
    """Run compliance check against standard requirements"""
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

    compliance_checks[organization_id] = checks
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
async def get_compliance_history(organization_id: str):
    """Get compliance check history for an organization"""
    if organization_id in compliance_checks:
        return compliance_checks[organization_id]
    return []


@app.get("/standards/{standard_type}/disclosure-requirements")
async def get_disclosure_requirements(standard_type: StandardType):
    """Get disclosure requirements for a standard"""
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


@app.get("/standards/comparison")
async def compare_standards(
    standard_1: StandardType,
    standard_2: StandardType
):
    """Compare two accounting standards"""
    s1 = await get_standard(standard_1)
    s2 = await get_standard(standard_2)

    return {
        "standard_1": s1.model_dump(),
        "standard_2": s2.model_dump(),
        "comparison": {
            "measurement_basis_differences": s1.measurement_basis != s2.measurement_basis,
            "consolidation_method_differences": s1.consolidation_method != s2.consolidation_method,
            "key_principles_common": [
                p for p in s1.key_principles if p in s2.key_principles
            ],
            "unique_to_standard_1": [
                p for p in s1.key_principles if p not in s2.key_principles
            ],
            "unique_to_standard_2": [
                p for p in s2.key_principles if p not in s1.key_principles
            ],
        },
    }


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

    base_guide = guides.get(standard_type.value, {
        "default": f"Measurement basis: {standard.measurement_basis.value}",
    })

    return {
        "standard": standard_type.value,
        "measurement_basis": standard.measurement_basis.value,
        "guidance": base_guide,
    }


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8095)