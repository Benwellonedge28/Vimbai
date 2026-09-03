"""
OpenAPI Configuration for Vimbai Services

This module provides shared OpenAPI configuration, custom schemas,
and documentation utilities for consistent API documentation across
all Vimbai microservices.

Usage:
    from openapi_config import get_openapi_schema, add_api_documentation

    # Apply custom OpenAPI configuration
    app.openapi = get_openapi_schema(app)
"""

from typing import Any, Dict, List, Optional

from fastapi import FastAPI
from fastapi.openapi.utils import get_openapi

# =============================================================================
# API METADATA
# =============================================================================

API_INFO = {
    "title": "Vimbai - Integrated Accounting & Finance Management System",
    "description": """
Vimbai is a comprehensive financial management system with microservices architecture.

## Features

* **Chart of Accounts**: Complete account hierarchy management
* **Double-Entry Accounting**: Journal entries, ledgers, trial balance
* **Financial Reporting**: Income statements, balance sheets, cash flow
* **NPO Fund Accounting**: Grant tracking, donor management, compliance
* **Bank Integration**: Reconciliation, transaction matching
* **Workflow Automation**: Approval processes, task scheduling

## Architecture

* **Database**: Neo4j Graph Database for relationship modeling
* **Authentication**: JWT/OAuth2 with RBAC
* **API Format**: RESTful JSON APIs
* **Documentation**: OpenAPI 3.0 / Swagger UI

## Rate Limits

* Default: 1000 requests/minute
* Authenticated: 5000 requests/minute
* Enterprise: Unlimited

## Support

* Email: support@vimbai.example.com
* Documentation: https://docs.vimbai.example.com
""",
    "version": "1.0.0",
    "contact": {
        "name": "Vimbai Support",
        "email": "support@vimbai.example.com",
        "url": "https://vimbai.example.com/support",
    },
    "license": {"name": "Business Source License 1.1 (no conversion period)", "url": "https://mariadb.com/bsl11/"},
    "terms_of_service": "https://vimbai.example.com/terms",
}

# External Documentation
EXTERNAL_DOCS = {"description": "Full API Documentation", "url": "https://docs.vimbai.example.com/api"}


# =============================================================================
# SECURITY SCHEMES
# =============================================================================

SECURITY_SCHEMES = {
    "BearerAuth": {
        "type": "http",
        "scheme": "bearer",
        "bearerFormat": "JWT",
        "description": "Enter your JWT token obtained from the identity service",
    },
    "ApiKeyAuth": {
        "type": "apiKey",
        "in": "header",
        "name": "X-API-Key",
        "description": "API key for service-to-service communication",
    },
}

# Security requirements for all endpoints
SECURITY = [{"BearerAuth": []}]


# =============================================================================
# TAG DEFINITIONS
# =============================================================================

TAGS_METADATA = [
    {
        "name": "accounts",
        "description": "Chart of Accounts management - create, read, update, delete accounts",
        "externalDocs": {
            "description": "Account management documentation",
            "url": "https://docs.vimbai.example.com/accounts",
        },
    },
    {
        "name": "journal-entries",
        "description": "Journal Entry operations - double-entry accounting transactions",
        "externalDocs": {
            "description": "Journal entries documentation",
            "url": "https://docs.vimbai.example.com/journal-entries",
        },
    },
    {"name": "ledgers", "description": "Ledger reports - account activity and balances"},
    {
        "name": "financial-statements",
        "description": "Financial statement generation - trial balance, income statement, balance sheet",
    },
    {
        "name": "special-journals",
        "description": "Special journal operations - sales, purchases, cash receipts/disbursements",
    },
    {"name": "subsidiary-ledgers", "description": "Subsidiary ledger reports - AR, AP, fixed assets, inventory"},
    {"name": "petty-cash", "description": "Petty cash fund management and tracking"},
    {"name": "bank-reconciliation", "description": "Bank statement reconciliation and matching"},
    {
        "name": "incomplete-records",
        "description": "Single-entry accounting for incomplete records - Statement of Affairs, Capital calculations",
    },
    {"name": "funds", "description": "NPO Fund Accounting - fund creation, transactions, restrictions"},
    {"name": "donations", "description": "Donation tracking and management"},
    {"name": "grants", "description": "Grant lifecycle management - application, approval, disbursement, reporting"},
    {"name": "donors", "description": "Donor information and contribution history"},
    {"name": "budgets", "description": "NPO budget planning and variance tracking"},
    {"name": "programs", "description": "NPO programs and associated metrics"},
    {"name": "projects", "description": "NPO project tracking and resource allocation"},
    {"name": "compliance", "description": "Internal controls, audit reports, and governance"},
    {"name": "impact", "description": "Impact measurement and SROI analysis"},
    {"name": "volunteers", "description": "Volunteer hours tracking and value calculation"},
    {"name": "health", "description": "Service health checks and status monitoring"},
]


# =============================================================================
# CUSTOM SCHEMAS
# =============================================================================

CUSTOM_SCHEMAS = {
    # Error schemas
    "ValidationError": {
        "title": "Validation Error",
        "type": "object",
        "properties": {
            "detail": {"type": "string", "description": "Human-readable error message"},
            "code": {"type": "string", "description": "Error code for programmatic handling"},
            "errors": {
                "type": "array",
                "items": {"type": "object", "properties": {"field": {"type": "string"}, "message": {"type": "string"}}},
            },
        },
    },
    "NotFoundError": {
        "title": "Resource Not Found",
        "type": "object",
        "properties": {"detail": {"type": "string"}, "code": {"type": "string", "example": "RESOURCE_NOT_FOUND"}},
    },
    "UnauthorizedError": {
        "title": "Authentication Required",
        "type": "object",
        "properties": {"detail": {"type": "string"}, "code": {"type": "string", "example": "UNAUTHORIZED"}},
    },
    # Pagination schemas
    "PaginatedResponse": {
        "title": "Paginated Response",
        "type": "object",
        "properties": {
            "items": {"type": "array", "description": "Array of result items"},
            "total": {"type": "integer", "description": "Total number of items matching query"},
            "page": {"type": "integer", "description": "Current page number"},
            "per_page": {"type": "integer", "description": "Items per page"},
            "pages": {"type": "integer", "description": "Total number of pages"},
        },
    },
    # Health check schema
    "HealthStatus": {
        "title": "Service Health Status",
        "type": "object",
        "properties": {
            "status": {
                "type": "string",
                "enum": ["healthy", "degraded", "unhealthy"],
                "description": "Overall service health",
            },
            "service": {"type": "string", "description": "Service name"},
            "version": {"type": "string", "description": "Service version"},
            "timestamp": {"type": "string", "format": "date-time", "description": "Health check timestamp"},
            "dependencies": {
                "type": "object",
                "description": "Status of service dependencies",
                "properties": {
                    "database": {"type": "string", "enum": ["healthy", "unhealthy"]},
                    "cache": {"type": "string", "enum": ["healthy", "unhealthy"]},
                },
            },
        },
    },
}


# =============================================================================
# UTILITY FUNCTIONS
# =============================================================================


def get_openapi_schema(app: FastAPI) -> Dict[str, Any]:
    """
    Generate complete OpenAPI schema for a Vimbai service.

    Args:
        app: FastAPI application instance

    Returns:
        OpenAPI schema dictionary
    """
    if app.title:
        openapi_schema = get_openapi(
            title=app.title,
            version=app.version,
            description=app.description,
            routes=app.routes,
        )
    else:
        openapi_schema = get_openapi(
            title="Vimbai Service",
            version="0.1.0",
            routes=app.routes,
        )

    # Add Vimbai-specific metadata
    openapi_schema.update(
        {
            "info": {
                **API_INFO,
                "title": app.title if app.title else API_INFO["title"],
                "version": app.version if app.version else API_INFO["version"],
            },
            "externalDocs": EXTERNAL_DOCS,
            "securitySchemes": SECURITY_SCHEMES,
            "tags": TAGS_METADATA,
            "components": {"schemas": CUSTOM_SCHEMAS},
        }
    )

    return openapi_schema


def add_api_documentation(app: FastAPI) -> None:
    """
    Configure FastAPI app with comprehensive OpenAPI documentation.

    Args:
        app: FastAPI application instance
    """
    # Set custom OpenAPI schema
    app.openapi = lambda: get_openapi_schema(app)

    # Add Swagger UI custom configuration
    if hasattr(app, "swagger_ui_parameters"):
        app.swagger_ui_parameters = {
            "deepLinking": True,
            "persistAuthorization": True,
            "displayRequestDuration": True,
            "docExpansion": "list",
            "filter": True,
            "showExtensions": True,
            "showCommonExtensions": True,
        }


def create_endpoint_documentation(
    summary: str,
    description: str,
    tags: List[str],
    response_model: Optional[type] = None,
    responses: Optional[Dict] = None,
    deprecated: bool = False,
) -> Dict[str, Any]:
    """
    Create documentation metadata for an endpoint.

    Args:
        summary: Brief description of endpoint
        description: Detailed explanation
        tags: List of tag names
        response_model: Pydantic model for response
        responses: Additional response definitions
        deprecated: Whether endpoint is deprecated

    Returns:
        Dictionary with endpoint documentation metadata
    """
    doc = {
        "summary": summary,
        "description": description,
        "tags": tags,
        "deprecated": deprecated,
        "responses": responses
        or {
            "401": {"description": "Authentication required"},
            "403": {"description": "Insufficient permissions"},
            "404": {"description": "Resource not found"},
        },
    }

    return doc


def generate_api_docs_html() -> str:
    """
    Generate custom HTML for API documentation portal.

    Returns:
        HTML string for custom documentation page
    """
    return """
    <!DOCTYPE html>
    <html>
    <head>
        <title>Vimbai API Documentation</title>
        <style>
            body { font-family: Arial, sans-serif; margin: 40px; }
            .header { background: #2c3e50; color: white; padding: 20px; border-radius: 8px; }
            .section { margin: 20px 0; padding: 15px; background: #f8f9fa; border-radius: 8px; }
            .endpoint { padding: 10px; margin: 5px 0; background: white; border-left: 4px solid #3498db; }
            .method { font-weight: bold; color: #2c3e50; }
            .path { color: #7f8c8d; }
            h2 { color: #2c3e50; }
            a { color: #3498db; }
        </style>
    </head>
    <body>
        <div class="header">
            <h1>Vimbai API Documentation</h1>
            <p>Version 1.0.0 | Last Updated: June 2024</p>
        </div>

        <div class="section">
            <h2>Quick Links</h2>
            <ul>
                <li><a href="/docs">Swagger UI</a> - Interactive API documentation</li>
                <li><a href="/redoc">ReDoc</a> - Alternative documentation view</li>
                <li><a href="/openapi.json">OpenAPI JSON</a> - Machine-readable schema</li>
            </ul>
        </div>

        <div class="section">
            <h2>Authentication</h2>
            <p>All API requests require authentication using JWT Bearer tokens:</p>
            <code>Authorization: Bearer &lt;your-jwt-token&gt;</code>
        </div>

        <div class="section">
            <h2>Services</h2>
            <ul>
                <li><a href="/accounting">Accounting Service</a> - Chart of accounts, journals, ledgers</li>
                <li><a href="/npo">NPO Service</a> - Fund accounting, grants, donor management</li>
                <li><a href="/banking">Banking Integration</a> - Bank reconciliation, transactions</li>
                <li><a href="/finance">Finance Service</a> - Forecasting, scenario analysis</li>
            </ul>
        </div>
    </body>
    </html>
    """


# =============================================================================
# EXPORTS
# =============================================================================

__all__ = [
    "get_openapi_schema",
    "add_api_documentation",
    "create_endpoint_documentation",
    "generate_api_docs_html",
    "API_INFO",
    "SECURITY_SCHEMES",
    "SECURITY",
    "TAGS_METADATA",
    "CUSTOM_SCHEMAS",
]
