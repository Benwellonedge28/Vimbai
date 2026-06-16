"""
FinAcc Cash Flow Statement Service
Generates cash flow statements for all business types and global accounting standards.
Supports IFRS (IAS 7), US GAAP (ASC 230), UK GAAP, and other regional standards.
"""

import os
import uuid
from datetime import datetime
from enum import Enum
from typing import Any, Dict, List, Optional

import httpx
import structlog
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

SERVICE_NAME = "cash-flow-statement-service"
SERVICE_VERSION = "1.0.0"
PORT = int(os.getenv("PORT", "8061"))
AUDIT_SERVICE_URL = os.getenv("AUDIT_SERVICE_URL", "http://localhost:8010")
ACCOUNTING_SERVICE_URL = os.getenv("ACCOUNTING_SERVICE_URL", "http://localhost:8000")

structlog.configure(
    processors=[structlog.stdlib.add_log_level, structlog.stdlib.add_logger_name,
                structlog.processors.TimeStamper(fmt="iso"), structlog.processors.JSONRenderer()],
    wrapper_class=structlog.stdlib.BoundLogger, context_class=dict,
    logger_factory=structlog.stdlib.LoggerFactory(), cache_logger_on_first_use=True,
)
logger = structlog.get_logger(SERVICE_NAME)

app = FastAPI(title="FinAcc Cash Flow Statement Service", version=SERVICE_VERSION, docs_url="/docs")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_credentials=True, allow_methods=["*"], allow_headers=["*"])


class AccountingStandard(str, Enum):
    # International
    IFRS = "IFRS"  # IAS 7 - International Accounting Standards
    US_GAAP = "US_GAAP"  # ASC 230 - Accounting Standards Codification
    UK_GAAP = "UK_GAAP"  # FRS 102 / FRS 105
    INDAS = "INDAS"  # Indian Accounting Standards
    CHINESE_ASBE = "CHINESE_ASBE"  # Chinese ASBE Standards
    JAPANESE_JGAAP = "JAPANESE_JGAAP"  # Japanese GAAP

    # Regional
    EU_ directives = "EU"
    GERMAN_HGB = "GERMAN_HGB"  # German Commercial Code
    FRENCH_PCG = "FRENCH_PCG"  # French Plan Comptable Général
    CANADIAN_CICA = "CANADIAN_CICA"  # Canadian Institute of Chartered Accountants
    AUSTRALIAN_AIFRS = "AUSTRALIAN_AIFRS"  # Australian IFRS
    NEW_ZEALAND_IFRS = "NZ_IFRS"  # New Zealand IFRS


class BusinessType(str, Enum):
    SOLE_TRADER = "sole_trader"
    PARTNERSHIP = "partnership"
    LIMITED_COMPANY = "limited_company"
    PUBLIC_LIMITED_COMPANY = "public_limited_company"
    CHARITY = "charity"
    COOPERATIVE = "cooperative"


class CashFlowMethod(str, Enum):
    DIRECT = "direct"  # Direct method
    INDIRECT = "indirect"  # Indirect method (default)


class CashFlowSection(str, Enum):
    OPERATING = "operating"
    INVESTING = "investing"
    FINANCING = "financing"


class CashFlowLineItem(BaseModel):
    line_code: str
    description: str
    amount: float = 0
    section: str
    standard_classification: str
    alternative_classifications: Dict[str, str] = {}  # standard -> classification


class CashFlowStatement(BaseModel):
    id: str = Field(default_factory=lambda: str(uuid.uuid4()))
    company_id: str
    business_type: str
    accounting_standard: str
    method: str  # direct or indirect
    period_start: datetime
    period_end: datetime
    opening_cash: float
    closing_cash: float

    # Operating activities
    operating_inflows: float = 0
    operating_outflows: float = 0
    operating_net: float = 0

    # Investing activities
    investing_inflows: float = 0
    investing_outflows: float = 0
    investing_net: float = 0

    # Financing activities
    financing_inflows: float = 0
    financing_outflows: float = 0
    financing_net: float = 0

    # Net change
    net_change: float = 0

    # Detailed line items
    line_items: List[CashFlowLineItem] = []

    # Standard-specific adjustments
    ifrs_adjustments: Dict[str, float] = {}  # IAS 7 specific items
    us_gaap_adjustments: Dict[str, float] = {}  # ASC 230 specific items
    uk_gaap_adjustments: Dict[str, float] = {}  # FRS 102 specific items

    # Reconciliation items
    reconciliation_items: Dict[str, float] = {}
    working_capital_changes: Dict[str, float] = {}

    status: str = "draft"
    created_at: datetime = Field(default_factory=datetime.utcnow)


class CashFlowTemplate(BaseModel):
    """Standard-specific templates for cash flow statement formatting"""
    standard: str
    business_type: str
    required_sections: List[str]
    required_line_items: List[Dict[str, Any]]
    optional_line_items: List[Dict[str, Any]]
    mandatory_disclosures: List[str]


# Standard-specific templates
CASH_FLOW_TEMPLATES = {
    "IFRS": {
        "sole_trader": {
            "operating": [
                {"code": "CF001", "item": "Cash receipts from customers", "ifrs": "Receipts from customers", "mandatory": True},
                {"code": "CF002", "item": "Cash paid to suppliers and employees", "ifrs": "Payments to suppliers and employees", "mandatory": True},
                {"code": "CF003", "item": "Cash generated from operations", "ifrs": "Cash generated from operations", "mandatory": True},
                {"code": "CF004", "item": "Interest paid", "ifrs": "Interest paid", "mandatory": True},
                {"code": "CF005", "item": "Income tax paid", "ifrs": "Income taxes paid", "mandatory": True},
            ],
            "investing": [
                {"code": "CF101", "item": "Purchase of property, plant and equipment", "ifrs": "Purchase of PPE", "mandatory": True},
                {"code": "CF102", "item": "Proceeds from sale of property, plant and equipment", "ifrs": "Proceeds from sale of PPE", "mandatory": True},
                {"code": "CF103", "item": "Purchase of investments", "ifrs": "Purchase of investments", "mandatory": False},
                {"code": "CF104", "item": "Proceeds from sale of investments", "ifrs": "Proceeds from sale of investments", "mandatory": False},
            ],
            "financing": [
                {"code": "CF201", "item": "Proceeds from loans", "ifrs": "Proceeds from borrowings", "mandatory": True},
                {"code": "CF202", "item": "Repayment of loans", "ifrs": "Repayment of borrowings", "mandatory": True},
                {"code": "CF203", "item": "Owner drawings", "ifrs": "Owner's capital", "mandatory": True},
                {"code": "CF204", "item": "Owner contributions", "ifrs": "Owner's capital", "mandatory": True},
            ]
        },
        "limited_company": {
            "operating": [
                {"code": "CF001", "item": "Profit before tax", "ifrs": "Profit before tax", "mandatory": True},
                {"code": "CF002", "item": "Adjustments for: Depreciation", "ifrs": "Depreciation and amortisation", "mandatory": True},
                {"code": "CF003", "item": "Adjustments for: Impairment", "ifrs": "Impairment of assets", "mandatory": True},
                {"code": "CF004", "item": "Adjustments for: Gain/Loss on disposal", "ifrs": "Gain/Loss on disposal", "mandatory": True},
                {"code": "CF005", "item": "Working capital changes", "ifrs": "Decrease/(Increase) in trade and other receivables", "mandatory": True},
                {"code": "CF006", "item": "Cash from operations", "ifrs": "Cash generated from operations", "mandatory": True},
                {"code": "CF007", "item": "Interest paid", "ifrs": "Interest paid", "mandatory": True},
                {"code": "CF008", "item": "Income tax paid", "ifrs": "Income taxes paid", "mandatory": True},
            ],
            "investing": [
                {"code": "CF101", "item": "Purchase of PPE", "ifrs": "Purchase of property, plant and equipment", "mandatory": True},
                {"code": "CF102", "item": "Proceeds from sale of PPE", "ifrs": "Proceeds from sale of PPE", "mandatory": True},
                {"code": "CF103", "item": "Purchase of intangible assets", "ifrs": "Purchase of intangible assets", "mandatory": True},
                {"code": "CF104", "item": "Acquisition of subsidiary", "ifrs": "Acquisition of subsidiary, net of cash", "mandatory": False},
                {"code": "CF105", "item": "Interest received", "ifrs": "Interest received", "mandatory": True},
                {"code": "CF106", "item": "Dividends received", "ifrs": "Dividends received", "mandatory": True},
            ],
            "financing": [
                {"code": "CF201", "item": "Proceeds from share issue", "ifrs": "Proceeds from issue of share capital", "mandatory": True},
                {"code": "CF202", "item": "Proceeds from borrowings", "ifrs": "Proceeds from borrowings", "mandatory": True},
                {"code": "CF203", "item": "Repayment of borrowings", "ifrs": "Repayment of borrowings", "mandatory": True},
                {"code": "CF204", "item": "Dividends paid", "ifrs": "Dividends paid", "mandatory": True},
                {"code": "CF205", "item": "Repurchase of shares", "ifrs": "Purchase of own shares", "mandatory": False},
            ]
        }
    },
    "US_GAAP": {
        "limited_company": {
            "operating": [
                {"code": "US001", "item": "Net income", "us_gaap": "Cash flows from operating activities", "mandatory": True},
                {"code": "US002", "item": "Depreciation and amortization", "us_gaap": "Depreciation and amortization", "mandatory": True},
                {"code": "US003", "item": "Stock-based compensation", "us_gaap": "Stock-based compensation expense", "mandatory": True},
                {"code": "US004", "item": "Deferred income taxes", "us_gaap": "Deferred income taxes", "mandatory": True},
                {"code": "US005", "item": "Changes in working capital", "us_gaap": "Changes in operating assets and liabilities", "mandatory": True},
            ],
            "investing": [
                {"code": "US101", "item": "Capital expenditures", "us_gaap": "Capital expenditures", "mandatory": True},
                {"code": "US102", "item": "Proceeds from asset sales", "us_gaap": "Proceeds from sale of property, plant and equipment", "mandatory": True},
                {"code": "US103", "item": "Acquisitions", "us_gaap": "Payments for acquisitions", "mandatory": False},
                {"code": "US104", "item": "Purchases of investments", "us_gaap": "Purchases of available-for-sale securities", "mandatory": False},
            ],
            "financing": [
                {"code": "US201", "item": "Proceeds from debt", "us_gaap": "Proceeds from issuance of debt", "mandatory": True},
                {"code": "US202", "item": "Repayment of debt", "us_gaap": "Repayments of debt", "mandatory": True},
                {"code": "US203", "item": "Proceeds from stock issuance", "us_gaap": "Proceeds from stock options exercised", "mandatory": True},
                {"code": "US204", "item": "Dividends paid", "us_gaap": "Dividends paid", "mandatory": True},
                {"code": "US205", "item": "Repurchase of common stock", "us_gaap": "Repurchase of common stock", "mandatory": False},
            ]
        }
    }
}

# Regional-specific disclosure requirements
REGIONAL_DISCLOSURES = {
    "IFRS": [
        "Significant non-cash transactions",
        "Components of cash and cash equivalents",
        "Restricted cash balances",
        "Bank overdrafts deducted from cash",
        "Additional line items for investing/financing",
    ],
    "US_GAAP": [
        "Supplemental disclosure of cash flow information",
        "Interest and income taxes paid",
        "Non-cash investing and financing activities",
        "Reconciliation of net income to operating cash flow",
    ],
    "UK_GAAP": [
        "Reconciliation of operating profit to net cash flow",
        "Analysis of changes in cash and debt",
        "Statement of total recognized gains and losses",
    ]
}

cash_flow_statements: List[CashFlowStatement] = []


async def call_accounting_service(method: str, endpoint: str, data: Optional[Dict] = None) -> Dict[str, Any]:
    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            url = f"{ACCOUNTING_SERVICE_URL}{endpoint}"
            if method == "POST":
                response = await client.post(url, json=data)
            else:
                response = await client.get(url)
            return response.json() if response.status_code in [200, 201] else {}
    except Exception:
        return {}


@app.get("/health")
async def health_check():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "status": "healthy"}


@app.get("/")
async def root():
    return {"service": SERVICE_NAME, "version": SERVICE_VERSION, "description": "Cash flow statement generation"}


@app.get("/standards")
async def list_accounting_standards():
    """List all supported accounting standards."""
    return {
        "standards": [
            {"code": s.value, "name": s.name, "description": get_standard_description(s.value)}
            for s in AccountingStandard
        ]
    }


@app.get("/standards/{standard}/template")
async def get_standard_template(standard: str, business_type: str):
    """Get cash flow template for a specific standard and business type."""
    template = CASH_FLOW_TEMPLATES.get(standard, {}).get(business_type)
    if not template:
        return {"error": "Template not found"}

    disclosures = REGIONAL_DISCLOSURES.get(standard, [])
    return {"template": template, "required_disclosures": disclosures}


@app.post("/statements/generate")
async def generate_cash_flow_statement(
    company_id: str, business_type: str, accounting_standard: str,
    method: str, period_start: datetime, period_end: datetime,
    opening_cash: float, line_items: List[Dict[str, Any]],
    working_capital_changes: Optional[Dict[str, float]] = None
):
    """Generate cash flow statement based on standard and business type."""
    statement = CashFlowStatement(
        company_id=company_id, business_type=business_type,
        accounting_standard=accounting_standard, method=method,
        period_start=period_start, period_end=period_end,
        opening_cash=opening_cash
    )

    if working_capital_changes:
        statement.working_capital_changes = working_capital_changes

    # Process line items
    for item in line_items:
        line_item = CashFlowLineItem(
            line_code=item.get("code", ""),
            description=item.get("description", ""),
            amount=item.get("amount", 0),
            section=item.get("section", ""),
            standard_classification=get_classification(item, accounting_standard),
            alternative_classifications=get_all_classifications(item)
        )
        statement.line_items.append(line_item)

        # Aggregate by section
        if line_item.section == "operating":
            if line_item.amount > 0:
                statement.operating_inflows += line_item.amount
            else:
                statement.operating_outflows += abs(line_item.amount)
        elif line_item.section == "investing":
            if line_item.amount > 0:
                statement.investing_inflows += line_item.amount
            else:
                statement.investing_outflows += abs(line_item.amount)
        elif line_item.section == "financing":
            if line_item.amount > 0:
                statement.financing_inflows += line_item.amount
            else:
                statement.financing_outflows += abs(line_item.amount)

    # Calculate net flows
    statement.operating_net = statement.operating_inflows - statement.operating_outflows
    statement.investing_net = statement.investing_inflows - statement.investing_outflows
    statement.financing_net = statement.financing_inflows - statement.financing_outflows
    statement.net_change = statement.operating_net + statement.investing_net + statement.financing_net
    statement.closing_cash = statement.opening_cash + statement.net_change

    # Add standard-specific adjustments
    if accounting_standard == "IFRS":
        add_ifrs_adjustments(statement)
    elif accounting_standard == "US_GAAP":
        add_us_gaap_adjustments(statement)
    elif accounting_standard == "UK_GAAP":
        add_uk_gaap_adjustments(statement)

    cash_flow_statements.append(statement)
    return statement


@app.post("/statements/{statement_id}/convert-standard")
async def convert_to_standard(statement_id: str, target_standard: str):
    """Convert existing statement to different accounting standard."""
    statement = next((s for s in cash_flow_statements if s.id == statement_id), None)
    if not statement:
        return {"error": "Statement not found"}

    converted_items = []
    for item in statement.line_items:
        new_classification = get_classification_for_standard(item, target_standard)
        new_item = CashFlowLineItem(
            line_code=item.line_code,
            description=new_classification if new_classification else item.description,
            amount=item.amount,
            section=item.section,
            standard_classification=new_classification,
            alternative_classifications={**item.alternative_classifications, target_standard: new_classification}
        )
        converted_items.append(new_item)

    statement.accounting_standard = target_standard
    statement.line_items = converted_items

    return statement


@app.get("/statements")
async def list_statements(
    company_id: Optional[str] = None,
    business_type: Optional[str] = None,
    accounting_standard: Optional[str] = None
):
    """List cash flow statements."""
    result = cash_flow_statements
    if company_id:
        result = [s for s in result if s.company_id == company_id]
    if business_type:
        result = [s for s in result if s.business_type == business_type]
    if accounting_standard:
        result = [s for s in result if s.accounting_standard == accounting_standard]
    return {"statements": result}


@app.get("/statements/{statement_id}")
async def get_statement(statement_id: str):
    """Get cash flow statement details."""
    statement = next((s for s in cash_flow_statements if s.id == statement_id), None)
    if not statement:
        return {"error": "Statement not found"}
    return statement


def get_standard_description(standard: str) -> str:
    """Get description for accounting standard."""
    descriptions = {
        "IFRS": "International Financial Reporting Standards (IAS 7)",
        "US_GAAP": "US Generally Accepted Accounting Principles (ASC 230)",
        "UK_GAAP": "UK Generally Accepted Accounting Practice (FRS 102)",
        "INDAS": "Indian Accounting Standards",
        "CHINESE_ASBE": "Chinese Accounting Standards for Business Enterprises",
        "JAPANESE_JGAAP": "Japanese Generally Accepted Accounting Principles",
        "GERMAN_HGB": "German Commercial Code (Handelsgesetzbuch)",
        "FRENCH_PCG": "French General Accounting Plan",
        "CANADIAN_CICA": "Canadian Institute of Chartered Accountants",
        "AUSTRALIAN_AIFRS": "Australian International Financial Reporting Standards",
        "NZ_IFRS": "New Zealand International Financial Reporting Standards",
    }
    return descriptions.get(standard, "Unknown standard")


def get_classification(item: Dict[str, Any], standard: str) -> str:
    """Get line item classification for specific standard."""
    template = CASH_FLOW_TEMPLATES.get(standard, {}).get(item.get("business_type", "limited_company"), {})

    for section in ["operating", "investing", "financing"]:
        for template_item in template.get(section, []):
            if template_item.get("code") == item.get("code"):
                return template_item.get(standard.lower(), template_item.get("item", ""))

    return item.get("description", "")


def get_all_classifications(item: Dict[str, Any]) -> Dict[str, str]:
    """Get classifications for all standards."""
    classifications = {}
    for standard in ["IFRS", "US_GAAP", "UK_GAAP"]:
        classifications[standard] = get_classification(item, standard)
    return classifications


def get_classification_for_standard(item: CashFlowLineItem, target_standard: str) -> str:
    """Get classification for target standard from item."""
    if target_standard in item.alternative_classifications:
        return item.alternative_classifications[target_standard]

    # Try to find from template
    for standard, classification in item.alternative_classifications.items():
        if classification:
            return classification

    return item.description


def add_ifrs_adjustments(statement: CashFlowStatement):
    """Add IFRS (IAS 7) specific adjustments and disclosures."""
    statement.ifrs_adjustments = {
        "interest_received_classification": "investing" if statement.investing_net < 0 else "operating",
        "dividend_received_classification": "investing" if statement.investing_net < 0 else "operating",
        "interest_paid_classification": "financing or operating",
        "tax_classification": "operating",
    }


def add_us_gaap_adjustments(statement: CashFlowStatement):
    """Add US GAAP (ASC 230) specific adjustments."""
    statement.us_gaap_adjustments = {
        "indirect_method_required": statement.method == "indirect",
        "reconciliation_required": True,
        "supplemental_disclosures_required": True,
        "interest_and_taxes_paid_disclosure": True,
    }


def add_uk_gaap_adjustments(statement: CashFlowStatement):
    """Add UK GAAP (FRS 102) specific adjustments."""
    statement.uk_gaap_adjustments = {
        "format_options": ["primary" or "secondary"],
        "notes_required": True,
        "cash_equivalents_definition": "liquid investments with maturity < 3 months",
    }


@app.get("/comparison/{company_id}")
async def compare_standards(company_id: str, period_start: datetime, period_end: datetime):
    """Compare cash flow statements across different standards."""
    company_statements = [s for s in cash_flow_statements
                         if s.company_id == company_id
                         and s.period_start == period_start
                         and s.period_end == period_end]

    if not company_statements:
        return {"error": "No statements found for comparison"}

    comparison = {
        "company_id": company_id,
        "period": {"start": period_start, "end": period_end},
        "standards_compared": [s.accounting_standard for s in company_statements],
        "key_metrics": {}
    }

    for statement in company_statements:
        comparison["key_metrics"][statement.accounting_standard] = {
            "operating_net": statement.operating_net,
            "investing_net": statement.investing_net,
            "financing_net": statement.financing_net,
            "net_change": statement.net_change,
            "closing_cash": statement.closing_cash
        }

    return comparison


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=PORT)