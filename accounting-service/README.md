# FinAcc Accounting Service

This is the Accounting Service microservice for the FinAcc application, built with FastAPI (Python) and utilizing Neo4j as its persistent data store. It is responsible for managing the Chart of Accounts, Journal Entries, Ledgers, and eventually generating Financial Statements.

## Features

- **Chart of Accounts (COA) Management:**
    - Create, Read (single, all), Update, Delete accounts.
    - Support for hierarchical accounts (parent/child relationships).
- Neo4j integration for flexible data modeling of accounting entities.

## Getting Started

To run this service along with Neo4j and the Identity Service, you need Docker and Docker Compose installed.

### 1. Prerequisites

- [Docker](https://docs.docker.com/get-docker/)
- [Docker Compose](https://docs.docker.com/compose/install/)

### 2. Environment Variables

The service requires the following environment variables. It's recommended to set these in a `.env` file at the root of the project where `docker-compose.yml` is located, or directly in `docker-compose.yml` for local development.

- `NEO4J_URI`: The connection URI for your Neo4j database (e.g., `bolt://neo4j:7687` when running with Docker Compose).
- `NEO4J_USER`: The username for Neo4j (e.g., `neo4j`).
- `NEO4J_PASSWORD`: The password for Neo4j (e.g., `neo4j` for default Docker setup, **CHANGE THIS IN PRODUCTION**).

### 3. Running the Services (with Docker Compose)

Navigate to the root directory of the FinAcc project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `identity-service` and `accounting-service` Docker images.
2.  Start a Neo4j container.
3.  Start the `identity-service` and `accounting-service` containers, connecting them to Neo4j.
4.  Ensure Neo4j schema constraints are created for both services.

The Accounting Service will be accessible at `http://localhost:8000`.

### 4. Interacting with the API (Chart of Accounts)

You can use tools like `curl`, Postman, or Insomnia to interact with the service.

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

### 5. Development

If developing locally (outside Docker Compose), ensure you have Python (3.10+) installed and your environment variables (`NEO4J_URI`, `NEO4J_USER`, `NEO4J_PASSWORD`) are correctly set.

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

## Future Enhancements

-   Implementation of Journal Entries and Journal Lines.
-   Integration with other FinAcc services (e.g., Identity Service for authentication).
-   Ledger posting and financial statement generation.
-   Support for specific accounting modalities (e.g., Fund Accounting dimensions).
