# Vimbai Developer Documentation

## Overview

Vimbai is a comprehensive financial management system built with microservices architecture. This guide provides detailed documentation for developers working with the Vimbai codebase.

## Table of Contents

1. [Architecture](#architecture)
2. [Services Overview](#services-overview)
3. [Database Schema](#database-schema)
4. [API Documentation](#api-documentation)
5. [Development Setup](#development-setup)
6. [Testing](#testing)
7. [Deployment](#deployment)
8. [Performance Optimization](#performance-optimization)

---

## Architecture

Vimbai follows a microservices architecture pattern with the following key components:

```
┌─────────────────────────────────────────────────────────────────┐
│                         API Gateway                              │
│                    (OAuth2 / JWT Auth)                          │
└───────────────────────────┬─────────────────────────────────────┘
                            │
    ┌───────────┬───────────┼───────────┬───────────┬───────────┐
    │           │           │           │           │           │
┌───▼───┐   ┌───▼───┐   ┌───▼───┐   ┌───▼───┐   ┌───▼───┐   ┌───▼───┐
│Account│   │  NPO  │   │Banking│   │Finance│   │ Fraud │   │Report│
│Service│   │Service│   │Service│   │Service│   │Detect │   │Service│
└───┬───┘   └───┬───┘   └───┬───┘   └───┬───┘   └───┬───┘   └───┬───┘
    │           │           │           │           │           │
    └───────────┴───────────┴───────────┼───────────┴───────────┘
                                       │
                            ┌──────────▼──────────┐
                            │     Neo4j Graph      │
                            │     Database        │
                            └─────────────────────┘
```

### Technology Stack

| Component | Technology |
|-----------|------------|
| Backend Framework | FastAPI (Python) |
| Database | Neo4j Graph Database |
| Authentication | JWT / OAuth2 |
| Containerization | Docker, Docker Compose |
| Message Queue | RabbitMQ (optional) |
| Monitoring | Prometheus, Grafana |
| Mobile | Flutter (Dart) |
| Web Frontend | React |

---

## Services Overview

### 1. Accounting Service (`accounting-service/`)

**Purpose**: Core accounting functionality - double-entry bookkeeping, financial statements.

**Key Features**:
- Chart of Accounts management
- Journal Entries (double-entry transactions)
- Ledger reports
- Trial Balance generation
- Income Statement
- Balance Sheet
- Special Journals (Sales, Purchases, Cash)
- Subsidiary Ledgers (AR, AP, Fixed Assets, Inventory)
- Petty Cash management
- Bank Reconciliation
- Incomplete Records (Single Entry System)

**Database Nodes**: Account, JournalEntry, LedgerEntry, TrialBalance, PettyCashFund, BankReconciliation

**Key Endpoints**:
```
POST   /accounts/                      - Create account
GET    /accounts/{number}              - Get account
POST   /journal-entries/               - Create journal entry
GET    /ledgers/{account_number}       - Get ledger
GET    /trial-balance/                  - Generate trial balance
GET    /income-statement/               - Generate income statement
GET    /balance-sheet/                  - Generate balance sheet
```

### 2. NPO Service (`npo-service/`)

**Purpose**: Non-Profit Organization accounting - fund accounting, grant management.

**Key Features**:
- Fund Accounting (General, Restricted, Endowment, Capital, Project)
- Net Assets classification (With/Without Donor Restrictions)
- Donation tracking
- Grant lifecycle (Application, Approval, Drawdowns, Reporting)
- Donor management
- Budget planning and variance analysis
- Project and Program tracking
- Internal controls and audit reports
- Impact measurement and SROI analysis
- Volunteer hours tracking

**Database Nodes**: NPOFund, Donor, Grant, Budget, Project, Program, AuditReport

**Key Endpoints**:
```
POST   /funds/                         - Create fund
POST   /donations/                     - Record donation
POST   /grants/                        - Create grant
POST   /budgets/                       - Create budget
GET    /statements/activities/          - Statement of Activities
GET    /statements/financial-position/  - Statement of Financial Position
```

### 3. Banking Integration Service (`banking-integration-service/`)

**Purpose**: Bank account integration and transaction reconciliation.

**Key Features**:
- Bank account management
- Transaction import
- Automatic reconciliation
- Transaction matching algorithms
- Bank statement parsing

### 4. Finance Service (`finance-service/`)

**Purpose**: Financial planning and analysis.

**Key Features**:
- Forecasting models
- Scenario analysis
- Budget vs Actual comparisons
- Cash flow projections

### 5. Fraud Detection Service (`fraud-detection-service/`)

**Purpose**: Anomaly detection and fraud prevention.

**Key Features**:
- ML-based transaction analysis
- Anomaly detection
- Pattern recognition
- Real-time alerts

### 6. Reporting Service (`reporting-service/`)

**Purpose**: Report generation and visualization.

**Key Features**:
- Custom report builder
- Scheduled reports
- Export capabilities (PDF, Excel)
- Dashboard creation

---

## Database Schema

### Neo4j Node Types

#### Core Accounting Nodes

```cypher
// Account node
(:Account {
    account_number: String,
    account_name: String,
    account_type: String,  // Asset, Liability, Equity, Revenue, Expense
    description: String,
    is_active: Boolean,
    is_control_account: Boolean,
    created_at: DateTime,
    user_id: String
})

// Journal Entry node
(:JournalEntry {
    entry_id: String,
    entry_date: DateTime,
    description: String,
    reference_number: String,
    status: String,  // draft, posted, void
    total_amount: Decimal,
    created_at: DateTime,
    user_id: String
})

// Ledger Entry node
(:LedgerEntry {
    entry_id: String,
    posting_date: DateTime,
    debit_amount: Decimal,
    credit_amount: Decimal,
    balance: Decimal,
    user_id: String
})
```

#### NPO Nodes

```cypher
// NPO Fund node
(:NPOFund {
    fund_id: String,
    fund_code: String,
    fund_name: String,
    fund_type: String,  // general, restricted, endowment, capital, project
    current_balance: Decimal,
    total_contributions: Decimal,
    total_disbursements: Decimal,
    user_id: String
})

// Donor node
(:Donor {
    donor_id: String,
    name: String,
    email: String,
    phone: String,
    address: String,
    preferred_donation_type: String,
    user_id: String
})

// Grant node
(:Grant {
    grant_id: String,
    grant_name: String,
    grantor: String,
    grant_amount: Decimal,
    amount_disbursed: Decimal,
    status: String,  // application, approved, active, completed
    start_date: Date,
    end_date: Date,
    user_id: String
})
```

### Key Relationships

```cypher
// User owns accounts
(u:User)-[:OWNS]->(a:Account)

// Journal entry posted to account
(j:JournalEntry)-[:POSTED_TO]->(a:Account)

// Donation to fund
(d:Donation)-[:DONATION_TO_FUND]->(f:NPOFund)

// Grant in fund
(g:Grant)-[:GRANT_IN_FUND]->(f:NPOFund)

// Donor makes donation
(dor:Donor)-[:MAKES_DONATION]->(d:Donation)

// Fund restricted by
(f:NPOFund)-[:HAS_RESTRICTION]->(r:FundRestriction)
```

---

## API Documentation

### Authentication

All API requests require a JWT Bearer token:

```bash
curl -X GET http://localhost:8000/accounts/ \
  -H "Authorization: Bearer <your-jwt-token>"
```

### Request/Response Format

**Request Body** (JSON):
```json
{
    "account_number": "ACC-1001",
    "account_name": "Cash",
    "account_type": "Asset",
    "description": "Cash on hand",
    "is_active": true
}
```

**Response** (JSON):
```json
{
    "id": "uuid-1234",
    "account_number": "ACC-1001",
    "account_name": "Cash",
    "account_type": "Asset",
    "current_balance": "0.00",
    "created_at": "2024-01-15T10:30:00Z"
}
```

### Error Responses

```json
{
    "detail": "Account not found",
    "code": "RESOURCE_NOT_FOUND"
}
```

### HTTP Status Codes

| Code | Meaning |
|------|---------|
| 200 | Success |
| 201 | Created |
| 400 | Bad Request |
| 401 | Unauthorized |
| 403 | Forbidden |
| 404 | Not Found |
| 422 | Validation Error |
| 500 | Internal Server Error |

---

## Development Setup

### Prerequisites

- Python 3.10+
- Docker and Docker Compose
- Neo4j Database
- Git

### Local Development

1. **Clone Repository**:
```bash
git clone https://github.com/Benwellonedge28/Vimbai
cd Vimbai
```

2. **Setup Virtual Environment**:
```bash
python -m venv venv
source venv/bin/activate  # Linux/Mac
venv\Scripts\activate     # Windows
```

3. **Install Dependencies**:
```bash
pip install -r requirements.txt
```

4. **Configure Environment**:
```bash
cp .env.example .env
# Edit .env with your settings
```

5. **Start Neo4j**:
```bash
docker-compose up -d neo4j
```

6. **Run Services**:
```bash
# Accounting Service
cd accounting-service
uvicorn main:app --reload --port 8001

# NPO Service (in another terminal)
cd npo-service
uvicorn main:app --reload --port 8002
```

### Running Tests

```bash
# Unit tests
pytest tests/

# Integration tests
pytest tests/integration/

# Performance tests
locust -f performance_tests.py --host=http://localhost:8001
```

---

## Testing

### Test Categories

1. **Unit Tests**: Test individual functions and classes
2. **Integration Tests**: Test service interactions
3. **Performance Tests**: Load and stress testing
4. **API Tests**: Endpoint testing with request/response validation

### Running Test Suites

```bash
# All tests
pytest

# With coverage
pytest --cov=. --cov-report=html

# Specific service tests
pytest tests/test_accounting_service.py

# Performance tests
locust -f performance_tests.py --headless -u 100 -r 10 -t 60s
```

---

## Deployment

### Docker Deployment

```bash
# Build all services
docker-compose build

# Start all services
docker-compose up -d

# Scale a service
docker-compose up -d --scale accounting-service=3
```

### Monitoring Setup

```bash
# Start monitoring stack
docker-compose -f monitoring/docker-compose.monitoring.yml up -d

# Access Prometheus
http://localhost:9090

# Access Grafana
http://localhost:3000 (admin/admin)
```

---

## Performance Optimization

### Database Indexes

Vimbai uses Neo4j indexes for optimal query performance. Indexes are automatically created on service startup.

See `database_optimization.py` for detailed index management.

### Key Indexes

```cypher
// Account lookups
CREATE INDEX account_number_index FOR (a:Account) ON (a.account_number)

// Date range queries
CREATE INDEX entry_date_index FOR (j:JournalEntry) ON (j.entry_date)

// User-scoped queries
CREATE INDEX user_id_index FOR (a:Account) ON (a.user_id)
```

### Query Optimization

Use the `QueryTemplates` class from `database_optimization.py` for pre-optimized Cypher queries.

---

## Monitoring & Alerting

### Prometheus Metrics

Access metrics at `/metrics` endpoint of each service:

```
# Request metrics
vimbai_http_requests_total
vimbai_http_request_duration_seconds

# Database metrics
vimbai_db_queries_total
vimbai_db_query_duration_seconds

# Business metrics
vimbai_transactions_total
vimbai_journal_entries_total
```

### Grafana Dashboards

Pre-configured dashboards available in `monitoring/grafana/dashboards/`.

### Alert Rules

Alert rules defined in `monitoring/alert_rules.yml`:

- High error rate (>5%)
- High latency (>2s p95)
- Database connection exhaustion
- Service down

---

## Troubleshooting

### Common Issues

1. **Database Connection Failed**
   - Check Neo4j is running: `docker-compose ps neo4j`
   - Verify credentials in `.env`

2. **Authentication Failed**
   - Check JWT token expiration
   - Verify API Gateway is running

3. **Slow Queries**
   - Check indexes are created
   - Review query execution plans
   - Enable query logging

### Logs

```bash
# View service logs
docker-compose logs accounting-service

# Follow logs
docker-compose logs -f accounting-service
```

---

## Contributing

1. Fork the repository
2. Create a feature branch
3. Write tests for new features
4. Submit pull request
5. Ensure CI/CD passes

---

## License

MIT License - See LICENSE file for details.

---

**Author**: Vimbai Development Team
**Version**: 1.0.0
**Last Updated**: June 2024