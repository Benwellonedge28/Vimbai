# NPO Service - Non-Profit Organization Accounting

A comprehensive microservices-based accounting service for Non-Profit Organizations (NPOs), NGOs, charities, trusts, and social enterprises.

## Overview

The NPO Service provides comprehensive accounting capabilities specifically designed for non-profit organizations, including:

- **Fund Accounting**: Separate tracking of General, Restricted, Endowment, Capital, and Project funds
- **Net Assets Management**: Tracking assets with and without donor restrictions
- **Revenue and Grant Management**: Donations, grants, membership fees, fundraising
- **Budget and Cost Allocation**: Budget creation, variance analysis, program costing
- **Project and Program Management**: Track projects, beneficiaries, outcomes
- **Donor Management**: Donor records, stewardship, lifetime value
- **Compliance and Governance**: Internal controls, audits, regulatory filings
- **Performance and Impact Measurement**: Program metrics, impact measurement, SROI

## Architecture

- **Framework**: FastAPI (Python)
- **Database**: Neo4j Graph Database
- **Authentication**: JWT-based via API Gateway
- **Pattern**: Microservices architecture

## Features

### 100 NPO Accounting Concepts Covered

1. **Fund Accounting** (Concepts 1-15)
   - General Fund, Restricted Fund, Endowment Fund, Capital Fund, Project Fund
   - Fund transactions with balance tracking
   - Donor-imposed and board-designated restrictions

2. **Net Assets** (Concepts 16-25)
   - With/Without Donor Restrictions
   - Accumulated Surplus/Deficit tracking
   - Net Assets changes over time

3. **Revenue and Income** (Concepts 26-50)
   - Donations (cash, stock, property, in-kind)
   - Grants (government, foundation, corporate)
   - Membership Fees and Subscriptions
   - Fundraising Revenue
   - Investment Income

4. **Assets and Liabilities** (Concepts 51-75)
   - Current and Fixed Assets
   - Endowment Assets
   - Depreciation tracking
   - Accrued Expenses and Deferred Revenue

5. **Financial Statements** (Concepts 76-100)
   - Statement of Financial Position
   - Statement of Activities
   - Statement of Cash Flows
   - Statement of Changes in Net Assets

6. **Budgeting and Control**
   - Budget creation and tracking
   - Variance analysis
   - Cost allocation
   - Program costing

7. **Compliance and Governance**
   - Internal Controls
   - Audit Reports (External/Internal)
   - Regulatory Compliance
   - Risk Management

8. **Performance and Impact**
   - Program Metrics
   - Impact Measurement
   - Social Return on Investment (SROI)
   - Volunteer Services Recognition

## API Endpoints

### Fund Accounting
- `POST /funds/` - Create fund
- `GET /funds/` - List all funds
- `GET /funds/{fund_id}` - Get fund by ID
- `POST /funds/{fund_id}/transactions/` - Create transaction
- `GET /funds/{fund_id}/transactions/` - List transactions
- `POST /funds/{fund_id}/restrictions/` - Add restriction

### Net Assets
- `POST /net-assets/` - Create net assets record
- `GET /net-assets/{as_of_date}` - Get net assets by date

### Revenue
- `POST /donations/` - Create donation
- `GET /donations/` - List donations
- `POST /grants/` - Create grant
- `GET /grants/` - List grants

### Projects & Programs
- `POST /projects/` - Create project
- `GET /projects/` - List projects
- `POST /programs/` - Create program
- `GET /programs/` - List programs

### Donors
- `POST /donors/` - Create donor
- `GET /donors/` - List donors

### Budgets
- `POST /budgets/` - Create budget
- `GET /budgets/` - List budgets
- `POST /budgets/{budget_id}/lines/` - Add budget line

### Compliance
- `POST /internal-controls/` - Create internal control
- `GET /internal-controls/` - List internal controls
- `POST /audit-reports/` - Create audit report
- `GET /audit-reports/` - List audit reports

### Performance
- `POST /program-metrics/` - Create program metric
- `POST /impact-measurements/` - Create impact measurement
- `POST /sroi-analyses/` - Create SROI analysis
- `POST /volunteer-records/` - Create volunteer record

### Financial Statements
- `POST /statements/activities/` - Create Statement of Activities
- `GET /statements/activities/` - Get Statement of Activities
- `POST /statements/financial-position/` - Create Statement of Financial Position

### Assets
- `POST /assets/` - Create NPO asset

## Installation

```bash
pip install -r requirements.txt
```

## Configuration

Create a `.env` file with:

```
NEO4J_URI=bolt://localhost:7687
NEO4J_USER=neo4j
NEO4J_PASSWORD=your_password
API_GATEWAY_URL=http://api-gateway:8081
```

## Running

```bash
uvicorn main:app --host 0.0.0.0 --port 8087 --reload
```

## Docker

```bash
docker build -t npo-service .
docker run -p 8087:8087 npo-service
```

## License

MIT