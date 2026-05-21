# FinAcc Accounting Service

This service serves as the core financial ledger for the FinAcc system. It manages the Chart of Accounts, processes Journal Entries, and generates fundamental financial reports like Ledgers, Trial Balances, Income Statements, and Balance Sheets.

## Features

-   **Chart of Accounts (COA) Management:** CRUD operations for defining and managing financial accounts.
-   **Journal Entry Management:** CRUD operations for recording financial transactions, ensuring double-entry accounting principles.
-   **Automated Journal Entry Validation:** Checks for balanced debits and credits.
-   **Fraud Detection Integration:** Automatically sends new journal entries to the Fraud Detection Service for anomaly detection.
-   **Vendor Bill Processing:** Automates the creation of journal entries for vendor bills based on Purchase Orders from the Supply Chain Service. (NEW)
-   **Ledger Generation:** Provides detailed transaction histories for individual accounts.
-   **Trial Balance Generation:** Summarizes all account balances to verify the equality of debits and credits.
-   **Financial Statement Generation:** Dynamically generates Income Statements and Balance Sheets based on recorded transactions.
-   **JWT Authentication and Role-Based Access Control (RBAC)** for all API endpoints.
-   **Robust Error Handling:** Standardized error responses with custom exceptions.

## Architecture

The Accounting Service is a FastAPI application that uses Neo4j as its primary data store for the Chart of Accounts and Journal Entries. It communicates with other FinAcc microservices (like Banking Integration, Multimodal Pipeline, and Finance Service, Supply Chain Service) via the API Gateway using `httpx`.

## Getting Started

To run this service along with Neo4j and other FinAcc services, you need Docker and Docker Compose installed.

### 1. Prerequisites

-   [Docker](https://docs.docker.com/get-docker/)
-   [Docker Compose](https://docs.docker.com/compose/install/)

### 2. Environment Variables

The service requires the following environment variables. It's recommended to set these in a `.env` file at the root of the project where `docker-compose.yml` is located.

-   `NEO4J_URI`: Connection URI for the Neo4j database (e.g., `bolt://neo4j:7687`).
-   `NEO4J_USER`: Username for Neo4j.
-   `NEO4J_PASSWORD`: Password for Neo4j.
-   `JWT_SECRET`: A strong, random secret key for signing JWT tokens. **CRITICAL: This MUST match the `JWT_SECRET` used by the `identity-service` for token validation to work.**
-   `API_GATEWAY_URL`: The internal URL of the API Gateway (e.g., `http://api-gateway:8081` when running with Docker Compose).

### 3. Running the Services (with Docker Compose)

Navigate to the root directory of the FinAcc project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `accounting-service` and other FinAcc Docker images.
2.  Start a Neo4j container and all other FinAcc microservices.

The Accounting Service API endpoints will be accessible via the API Gateway at `http://localhost:8081`.

### 4. Interacting with the API (Authenticated Endpoints)

All endpoints require a valid JWT in the `Authorization: Bearer <token>` header.

#### **Step 1: Get a JWT Token**

Refer to the `identity-service/README.md` for instructions on how to register a user and obtain a JWT token.
Ensure the user has relevant `accounting.*` permissions, or `SUPER_ADMIN` role.

#### **Step 2: Make Authenticated Requests via API Gateway**

Use the copied JWT token in the `Authorization` header for all requests.

**Header:** `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`

---

### **Journal Entry Endpoints**

#### Create a New Journal Entry
**Endpoint:** `POST http://localhost:8081/journal-entries/` (via API Gateway)
**Permissions:** `accounting.write.journal_entries`

*Upon creation, this journal entry will automatically be sent to the Fraud Detection Service for analysis.* 

# ... (rest of the Journal Entry Endpoints are unchanged) ...

---

### **Vendor Bill Endpoints (NEW)**

#### Create a Vendor Bill from a Purchase Order
**Endpoint:** `POST http://localhost:8081/vendor-bills/` (via API Gateway)
**Permissions:** `accounting.create.vendor_bill`
**Body (JSON):**
```json
{
  "purchase_order_id": "uuid-of-purchase-order",
  "bill_date": "2026-05-25T09:00:00Z",
  "due_date": "2026-06-25T00:00:00Z",
  "additional_lines": [
    {
      "account_number": "6100",
      "debit": 50.00,
      "credit": 0.00,
      "description": "Shipping Expense"
    }
  ]
}
```
*This endpoint will fetch the Purchase Order details from the Supply Chain Service and automatically create a corresponding Journal Entry.* 

# ... (rest of the README.md content is unchanged) ...
