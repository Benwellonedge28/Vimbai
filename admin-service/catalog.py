"""Code-defined feature/config catalogs (immutable seeds; caller overrides persist in Neo4j)."""

from typing import Any, Dict

from admin_service.models import Feature, FeatureCategory, FeatureDependency, FeatureStatus, ServiceHealth, SystemConfig

# ============================================================================
# Feature Registry (platform catalog — status overrides are per-caller/Book)
# ============================================================================

FEATURES: Dict[str, Feature] = {
    # Accounting Features
    "double_entry": Feature(
        id="double_entry",
        name="Double-Entry Accounting",
        description="Enable double-entry bookkeeping with debit/credit validation",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
        requires_permission="accounting.double_entry",
    ),
    "single_entry": Feature(
        id="single_entry",
        name="Single-Entry System",
        description="Enable single-entry (incomplete records) accounting",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
        requires_permission="accounting.single_entry",
    ),
    "fund_accounting": Feature(
        id="fund_accounting",
        name="Fund Accounting",
        description="Enable fund-based accounting for nonprofits/government",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
        requires_permission="accounting.fund",
    ),
    "project_accounting": Feature(
        id="project_accounting",
        name="Project Accounting",
        description="Enable project/cost center tracking",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
        requires_permission="accounting.project",
    ),
    "npo_accounting": Feature(
        id="npo_accounting",
        name="NPO Accounting",
        description="Enable nonprofit organization specific features",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
        requires_permission="accounting.npo",
    ),
    "depreciation_tracking": Feature(
        id="depreciation_tracking",
        name="Fixed Asset Depreciation",
        description="Enable automatic depreciation calculation for fixed assets",
        category=FeatureCategory.ACCOUNTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    # Finance Features
    "budgeting": Feature(
        id="budgeting",
        name="Budget Management",
        description="Enable budget creation, tracking, and variance analysis",
        category=FeatureCategory.FINANCE,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "scenario_modeling": Feature(
        id="scenario_modeling",
        name="What-If Scenario Modeling",
        description="Enable financial scenario creation and comparison",
        category=FeatureCategory.FINANCE,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "forecasting": Feature(
        id="forecasting",
        name="Cash Flow Forecasting",
        description="Enable AI-assisted cash flow forecasting",
        category=FeatureCategory.FINANCE,
        status=FeatureStatus.BETA,
        enabled_by_default=False,
    ),
    # Banking Features
    "bank_integration": Feature(
        id="bank_integration",
        name="Bank Feed Integration",
        description="Enable automatic bank feed imports and reconciliation",
        category=FeatureCategory.BANKING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "pos_integration": Feature(
        id="pos_integration",
        name="POS Integration",
        description="Enable Point-of-Sale system integration",
        category=FeatureCategory.BANKING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    # Fraud Detection
    "fraud_detection": Feature(
        id="fraud_detection",
        name="Real-time Fraud Detection",
        description="Enable ML-based fraud detection on transactions",
        category=FeatureCategory.FRAUD_DETECTION,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "fraud_alerts": Feature(
        id="fraud_alerts",
        name="Fraud Alert Notifications",
        description="Enable real-time fraud alert notifications",
        category=FeatureCategory.FRAUD_DETECTION,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    # Reporting
    "custom_reports": Feature(
        id="custom_reports",
        name="Custom Report Builder",
        description="Enable drag-and-drop report builder",
        category=FeatureCategory.REPORTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "pdf_export": Feature(
        id="pdf_export",
        name="PDF Export",
        description="Enable PDF export for reports",
        category=FeatureCategory.REPORTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "excel_export": Feature(
        id="excel_export",
        name="Excel Export",
        description="Enable Excel export for reports",
        category=FeatureCategory.REPORTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "financial_statements": Feature(
        id="financial_statements",
        name="Financial Statement Generation",
        description="Enable automatic income statement, balance sheet, cash flow",
        category=FeatureCategory.REPORTING,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    # Workflow
    "approval_workflows": Feature(
        id="approval_workflows",
        name="Approval Workflows",
        description="Enable configurable approval chains",
        category=FeatureCategory.WORKFLOW,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "audit_trail": Feature(
        id="audit_trail",
        name="Immutable Audit Trail",
        description="Track all changes with immutable audit log",
        category=FeatureCategory.WORKFLOW,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    # Multimodal
    "ocr_processing": Feature(
        id="ocr_processing",
        name="OCR Document Processing",
        description="Enable OCR for scanned documents",
        category=FeatureCategory.MULTIMODAL,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "voice_input": Feature(
        id="voice_input",
        name="Voice Input",
        description="Enable voice-to-journal-entry feature",
        category=FeatureCategory.MULTIMODAL,
        status=FeatureStatus.BETA,
        enabled_by_default=False,
    ),
    # Security
    "oauth_login": Feature(
        id="oauth_login",
        name="OAuth2/OIDC Login",
        description="Enable social login (Google, GitHub, Microsoft)",
        category=FeatureCategory.SECURITY,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "mfa": Feature(
        id="mfa",
        name="Multi-Factor Authentication",
        description="Enable TOTP-based MFA",
        category=FeatureCategory.SECURITY,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "rate_limiting": Feature(
        id="rate_limiting",
        name="API Rate Limiting",
        description="Enable rate limiting on API endpoints",
        category=FeatureCategory.SECURITY,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    # System
    "offline_mode": Feature(
        id="offline_mode",
        name="Offline-First Mode",
        description="Enable offline data entry and sync",
        category=FeatureCategory.SYSTEM,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "graphql_api": Feature(
        id="graphql_api",
        name="GraphQL API",
        description="Enable GraphQL API endpoint",
        category=FeatureCategory.SYSTEM,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "websocket_alerts": Feature(
        id="websocket_alerts",
        name="Real-time WebSocket Alerts",
        description="Enable WebSocket for real-time notifications",
        category=FeatureCategory.SYSTEM,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
    "multi_currency": Feature(
        id="multi_currency",
        name="Multi-Currency Support",
        description="Enable multi-currency transactions and conversion",
        category=FeatureCategory.SYSTEM,
        status=FeatureStatus.ENABLED,
        enabled_by_default=True,
    ),
}

# ============================================================================
# Feature Dependencies (platform catalog)
# ============================================================================


FEATURE_DEPENDENCIES: Dict[str, FeatureDependency] = {
    "forecasting": FeatureDependency(
        feature_id="forecasting",
        depends_on=["budgeting", "scenario_modeling"],
        required_permissions=["finance.forecasting"],
        min_rollout_percentage=50,
    ),
    "voice_input": FeatureDependency(
        feature_id="voice_input",
        depends_on=["ocr_processing"],
        required_permissions=["multimodal.voice"],
        min_rollout_percentage=25,
    ),
    "approval_workflows": FeatureDependency(
        feature_id="approval_workflows",
        depends_on=["audit_trail"],
        required_permissions=["workflow.approval"],
        min_rollout_percentage=10,
    ),
}

# ============================================================================
# System Configuration Defaults (per-caller/Book overrides persist in Neo4j)
# ============================================================================


SYSTEM_CONFIG: Dict[str, SystemConfig] = {
    "company_name": SystemConfig(
        key="company_name",
        value="Vimbai Corporation",
        description="Company name displayed in reports",
        category="general",
    ),
    "fiscal_year_start": SystemConfig(
        key="fiscal_year_start",
        value="January",
        description="Start month of fiscal year",
        category="accounting",
    ),
    "base_currency": SystemConfig(
        key="base_currency",
        value="USD",
        description="Primary currency for financial statements",
        category="accounting",
    ),
    "date_format": SystemConfig(
        key="date_format",
        value="YYYY-MM-DD",
        description="Date format for displays",
        category="general",
    ),
    "timezone": SystemConfig(
        key="timezone",
        value="UTC",
        description="System timezone",
        category="general",
    ),
    "session_timeout_minutes": SystemConfig(
        key="session_timeout_minutes",
        value=60,
        description="Session timeout in minutes",
        category="security",
        is_sensitive=False,
    ),
    "max_login_attempts": SystemConfig(
        key="max_login_attempts",
        value=5,
        description="Maximum failed login attempts before lockout",
        category="security",
    ),
    "maintenance_mode": SystemConfig(
        key="maintenance_mode",
        value=False,
        description="Enable system maintenance mode",
        category="system",
    ),
}

# ============================================================================
# Service Health Registry (platform catalog)
# ============================================================================

SERVICES_HEALTH = [
    ServiceHealth(
        service_name="accounting-service",
        status="healthy",
        version="1.0.0",
        endpoints={"api": "http://localhost:8000"},
    ),
    ServiceHealth(
        service_name="finance-service",
        status="healthy",
        version="1.0.0",
        endpoints={"api": "http://localhost:8001"},
    ),
    ServiceHealth(
        service_name="identity-service",
        status="healthy",
        version="1.0.0",
        endpoints={"api": "http://localhost:8080"},
    ),
    ServiceHealth(
        service_name="audit-service",
        status="healthy",
        version="1.0.0",
        endpoints={"api": "http://localhost:8091"},
    ),
    ServiceHealth(
        service_name="api-gateway",
        status="healthy",
        version="1.0.0",
        endpoints={"api": "http://localhost:8081"},
    ),
    ServiceHealth(
        service_name="alerts-service",
        status="healthy",
        version="1.0.0",
        endpoints={"api": "http://localhost:8090"},
    ),
    ServiceHealth(
        service_name="notifications-service",
        status="healthy",
        version="1.0.0",
        endpoints={"api": "http://localhost:8091"},
    ),
    ServiceHealth(
        service_name="message-bus-service",
        status="healthy",
        version="1.0.0",
        endpoints={"api": "http://localhost:8097"},
    ),
    ServiceHealth(
        service_name="automation-engine",
        status="healthy",
        version="1.0.0",
        endpoints={"api": "http://localhost:8098"},
    ),
]
