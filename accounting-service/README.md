# FinAcc Accounting Service

This is the Accounting Service microservice for the FinAcc application, built with FastAPI (Python) and utilizing Neo4j as its persistent data store. It is responsible for managing the Chart of Accounts, Journal Entries, Ledgers, and eventually generating Financial Statements.

## Features

- **Chart of Accounts (COA) Management:**
    - Create, Read (single, all), Update, Delete accounts.
    - Support for hierarchical accounts (parent/child relationships).
- **Journal Entry Management:**
    - Create, Read (single, all), Delete journal entries and their associated lines.
    - Ensures double-entry bookkeeping principles (debits = credits) and links entries to accounts.
- **Ledger & Trial Balance Generation:**
    - Calculate current balance for individual ledger accounts.
    - Generate a consolidated Trial Balance report.
- **Financial Statement Generation:**
    - Generate Income Statements for a period.
    - Generate Balance Sheets as of a specific date.
    - Generate Cash Flow Statements for a period (simplified indirect method).
- Neo4j integration for flexible data modeling of accounting entities.
- **JWT Authentication and Role-Based Access Control (RBAC)** for all API endpoints.

## Getting Started

To run this service along with Neo4j and the Identity Service, you need Docker and Docker Compose installed.

### 1. Prerequisites

- [Docker](https://docs.docker.com/get-docker/)
- [Docker Compose](https://docs.docker.com/compose/install/)

### 2. Environment Variables

The services require the following environment variables. It's recommended to set these in a `.env` file at the root of the project where `docker-compose.yml` is located, or directly in `docker-compose.yml` for local development.

- `NEO4J_URI`: The connection URI for your Neo4j database (e.g., `bolt://neo4j:7687` when running with Docker Compose).
- `NEO4J_USER`: The username for Neo4j (e.g., `neo4j`).
- `NEO4J_PASSWORD`: The password for Neo4j (e.g., `neo4j` for default Docker setup, **CHANGE THIS IN PRODUCTION**).
- `JWT_SECRET`: A strong, random secret key for signing JWT tokens. **CRITICAL: This MUST match the `JWT_SECRET` used by the `identity-service` for token validation to work.**

### 3. Running the Services (with Docker Compose)

Navigate to the root directory of the FinAcc project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `identity-service`, `accounting-service`, and `finance-service` Docker images.
2.  Start a Neo4j container.
3.  Start all three microservices, connecting them to Neo4j.
4.  Ensure Neo4j schema constraints are created for all services, and initial roles are seeded.

The Accounting Service will be accessible at `http://localhost:8000`.

### 4. Interacting with the API (Authenticated Endpoints)

All endpoints now require a valid JWT in the `Authorization: Bearer <token>` header.

#### **Step 1: Get a JWT Token**

First, register and log in via the Identity Service:

1.  **Register a new user (if you haven't already):**
    **Endpoint:** `POST http://localhost:8080/register`
    **Body (JSON):**
    ```json
    {
      "username": "accountant_user",
      "email": "accountant@finacc.com",
      "password": "password123",
      "role_name": "ACCOUNTANT"
    }
    ```
    (Or `SUPER_ADMIN` for full access)

2.  **Login to get a token:**
    **Endpoint:** `POST http://localhost:8080/login`
    **Body (JSON):**
    ```json
    {
      "username": "accountant_user",
      "password": "password123"
    }
    ```
    This will return a `{"token": "eyJ..."}`. Copy this token.

#### **Step 2: Make Authenticated Requests to Accounting Service**

Use the copied JWT token in the `Authorization` header for all requests to the Accounting Service.

**Header:** `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`

**Permissions:**
- `ACCOUNTANT` role has `accounting.*` permissions (or more granular `accounting.write.accounts`, `accounting.read.accounts`, `accounting.read.journal_entries`, `accounting.write.journal_entries`, `accounting.read.ledger`, `accounting.read.trial_balance`, `accounting.read.financial_statements`, etc.), allowing operations on accounts, journal entries, ledgers, trial balance, and financial statements.
- `SUPER_ADMIN` role has `*.*` permissions, allowing all operations.

---

### **Chart of Accounts (COA) Endpoints**

**(Requires `accounting.read.accounts` or `accounting.write.accounts` or `accounting.delete.accounts` permissions)**

#### Create a new Account

**Endpoint:** `POST http://localhost:8000/accounts/`
**Body (JSON):**
```json
{
  "account_number": "1010",
  "account_name": "Cash (Operating)",
  "account_type": "Asset",
  "normal_balance": "Debit",
  "description": "Main bank account for daily operations",
  "parent_account_number": null
}
```
Or for a child account:
```json
{
  "account_number": "1100",
  "account_name": "Accounts Receivable",
  "account_type": "Asset",
  "normal_balance": "Debit",
  "description": "Amounts owed by customers",
  "parent_account_number": "1000" // Assuming 1000 is a parent account like "Current Assets"
}
```

#### Get all Accounts

**Endpoint:** `GET http://localhost:8000/accounts/`

#### Get Account by Number

**Endpoint:** `GET http://localhost:8000/accounts/{account_number}`
Example: `GET http://localhost:8000/accounts/1010`

#### Update an Account

**Endpoint:** `PUT http://localhost:8000/accounts/{account_number}`
Example: `PUT http://localhost:8000/accounts/1010`

**Body (JSON - partial update):**
```json
{
  "account_name": "Cash - Main Operating Account"
}
```

#### Delete an Account

**Endpoint:** `DELETE http://localhost:8000/accounts/{account_number}`
Example: `DELETE http://localhost:8000/accounts/1010`

---

### **Journal Entry Endpoints**

**(Requires `accounting.read.journal_entries` or `accounting.write.journal_entries` or `accounting.delete.journal_entries` permissions)**

#### Create a new Journal Entry

**Endpoint:** `POST http://localhost:8000/journal-entries/`
**Body (JSON):**
```json
{
  "entry_date": "2026-05-20T10:00:00Z",
  "description": "Recorded payment for office rent.",
  "reference_number": "RENT-05-26",
  "source_module": "Manual",
  "lines": [
    {
      "account_number": "6000",
      "debit": 1500.00,
      "credit": 0.00,
      "description": "Office Rent Expense"
    },
    {
      "account_number": "1010",
      "debit": 0.00,
      "credit": 1500.00,
      "description": "Cash (Bank Account) paid"
    }
  ]
}
```
*Note: Ensure accounts 6000 and 1010 exist in your Chart of Accounts before creating this entry.*

#### Get Journal Entry by ID

**Endpoint:** `GET http://localhost:8000/journal-entries/{entry_id}`
Example: `GET http://localhost:8000/journal-entries/a1b2c3d4-e5f6-7890-1234-567890abcdef`

#### Get all Journal Entries

**Endpoint:** `GET http://localhost:8000/journal-entries/`

#### Delete a Journal Entry

**Endpoint:** `DELETE http://localhost:8000/journal-entries/{entry_id}`
Example: `DELETE http://localhost:8000/journal-entries/a1b2c3d4-e5f6-7890-1234-567890abcdef`

---

### **Ledger and Trial Balance Endpoints**

**(Requires `accounting.read.ledger` or `accounting.read.trial_balance` permissions)**

#### Get Ledger Account Balance

**Endpoint:** `GET http://localhost:8000/ledger/{account_number}`
Example: `GET http://localhost:8000/ledger/1010`

#### Generate Trial Balance

**Endpoint:** `GET http://localhost:8000/trial-balance/`

---

### **Financial Statement Endpoints**

**(Requires `accounting.read.financial_statements` permissions)**

#### Get Income Statement

**Endpoint:** `GET http://localhost:8000/financial-statements/income-statement`
**Query Parameters:**
- `start_date`: e.g., `2026-01-01T00:00:00Z`
- `end_date`: e.g., `2026-03-31T23:59:59Z`

Example: `GET http://localhost:8000/financial-statements/income-statement?start_date=2026-01-01T00:00:00Z&end_date=2026-03-31T23:59:59Z`

#### Get Balance Sheet

**Endpoint:** `GET http://localhost:8000/financial-statements/balance-sheet`
**Query Parameters:**
- `as_of_date`: e.g., `2026-03-31T23:59:59Z`

Example: `GET http://localhost:8000/financial-statements/balance-sheet?as_of_date=2026-03-31T23:59:59Z`

#### Get Cash Flow Statement

**Endpoint:** `GET http://localhost:8000/financial-statements/cash-flow-statement`
**Query Parameters:**
- `start_date`: e.g., `2026-01-01T00:00:00Z`
- `end_date`: e.g., `2026-03-31T23:59:59Z`

Example: `GET http://localhost:8000/financial-statements/cash-flow-statement?start_date=2026-01-01T00:00:00Z&end_date=2026-03-31T23:59:59Z`

---

### 5. Development

If developing locally (outside Docker Compose), ensure you have Python (3.10+) installed and your environment variables (`NEO4J_URI`, `NEO4J_USER`, `NEO4J_PASSWORD`, `JWT_SECRET`) are correctly set.

```bash
# Navigate to accounting-service directory
cd accounting-service

# Install dependencies
pip install -r requirements.txt

# Run locally
uvicorn main:app --host 0.0.0.0 --port 8000
```

## Database Model

Accounts are stored as `(:Account)` nodes in Neo4j with properties like `id`, `account_number` (unique), `account_name` (unique), `account_type`, `normal_balance`, `description`, `created_at`, `updated_at`.
Hierarchical relationships are modeled using `(:Account)-[:HAS_PARENT]->(:Account)`.
Unique constraints are enforced on `Account.account_number` and `Account.account_name`.

Journal entries are stored as `(:JournalEntry)` nodes with properties `id`, `entry_date`, `description`, `reference_number`, `source_module`, `created_at`, `updated_at`.
Each journal entry has multiple `(:JournalLine)` nodes linked via `(:JournalEntry)-[:HAS_LINE]->(:JournalLine)`.
Each `(:JournalLine)` node has properties `id`, `debit`, `credit`, `description` and is linked to an `(:Account)` node via `(:JournalLine)-[:AFFECTS]->(:Account)`.

## Future Enhancements

-   Refined Cash Flow Statement generation (e.g., direct method, more detailed indirect adjustments).
-   Integration with other FinAcc services (e.g., Finance Service for budget vs. actual comparisons).
-   Advanced error handling and validation for real-world accounting scenarios.
-   Caching for frequently accessed reports.
