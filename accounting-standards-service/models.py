"""Pydantic models and enums for Accounting Standards Service."""

from datetime import date, datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Literal, Optional

from pydantic import BaseModel, Field


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
