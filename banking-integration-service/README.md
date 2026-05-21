# FinAcc Banking Integration Service

This service facilitates connecting to various financial institutions, retrieving bank account and transaction data, and integrating this data with the core FinAcc Accounting Service for automated reconciliation and journal entry creation.

## Features

-   **Bank Account Management:** CRUD operations for connecting and managing bank accounts.
-   **Transaction Retrieval:** Simulates fetching transaction data from connected banks.
-   **Transaction Categorization:** Supports categorization of transactions.
-   **Automated Journal Entry Creation:** Can create corresponding journal entries in the Accounting Service.
-   **Fraud Detection Integration:** Automatically sends new transactions to the Fraud Detection Service for analysis upon creation. (NEW)
-   **JWT Authentication and Role-Based Access Control (RBAC)** for all API endpoints.
-   **Robust Error Handling:** Standardized error responses with custom exceptions.

## Architecture

The Banking Integration Service is a FastAPI application that uses Neo4j to store bank and bank account details, as well as transaction records. It communicates with the Accounting Service (for journal entries) and the Fraud Detection Service (for transaction analysis) via the API Gateway using `httpx`.

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
1.  Build the `banking-integration-service` and other FinAcc Docker images.
2.  Start a Neo4j container and all other FinAcc microservices.

The Banking Integration Service API endpoints will be accessible via the API Gateway at `http://localhost:8081`.

### 4. Interacting with the API (Authenticated Endpoints)

All endpoints require a valid JWT in the `Authorization: Bearer <token>` header.

#### **Step 1: Get a JWT Token**

Refer to the `identity-service/README.md` for instructions on how to register a user and obtain a JWT token.
Ensure the user has relevant `banking.*` permissions, or `SUPER_ADMIN` role.

#### **Step 2: Make Authenticated Requests via API Gateway**

Use the copied JWT token in the `Authorization` header for all requests.

**Header:** `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`

---

### **Transaction Endpoints**

#### Create a New Transaction

**Endpoint:** `POST http://localhost:8081/transactions/` (via API Gateway)
**Permissions:** `banking.write.transactions`
**Body (JSON):**
```json
{
  "bank_account_id": "uuid-of-bank-account",
  "transaction_id": "BANK-TRANS-12345",
  "description": "Payment to Vendor X",
  "amount": -150.00,
  "currency": "USD",
  "transaction_date": "2026-05-20T10:00:00Z",
  "post_date": "2026-05-21T09:00:00Z",
  "category": "Supplies",
  "accounting_account_number": "6010"
}
```
*Upon creation, this transaction will automatically be sent to the Fraud Detection Service for analysis.* 

#### Get a Specific Transaction

**Endpoint:** `GET http://localhost:8081/transactions/{transaction_id}` (via API Gateway)
**Permissions:** `banking.read.transactions`

#### Get All Transactions

**Endpoint:** `GET http://localhost:8081/transactions/` (via API Gateway)
**Permissions:** `banking.read.transactions`
Query parameters: `bank_account_id`, `start_date`, `end_date` for filtering.

#### Update an Existing Transaction

**Endpoint:** `PUT http://localhost:8081/transactions/{transaction_id}` (via API Gateway)
**Permissions:** `banking.write.transactions`

#### Delete an Existing Transaction

**Endpoint:** `DELETE http://localhost:8081/transactions/{transaction_id}` (via API Gateway)
**Permissions:** `banking.delete.transactions`

#### Create Journal Entry for a Transaction

**Endpoint:** `POST http://localhost:8081/transactions/{transaction_id}/create-journal-entry` (via API Gateway)
**Permissions:** `banking.create.journal_entry`
**Query Parameters:**
-   `debit_account_number`: e.g., "5000"
-   `credit_account_number`: e.g., "1010"

---

## Error Handling

The service employs custom exceptions and global exception handlers to provide structured and informative error responses:
```json
{
  "detail": "Descriptive error message",
  "code": "ERROR_CODE_ENUM",
  "status_code": 404
}
```
Common error codes include: `NOT_FOUND`, `CONFLICT_ERROR`, `VALIDATION_ERROR`, `UNAUTHORIZED`, `FORBIDDEN`, `BANK_ACCOUNT_NOT_FOUND`, `TRANSACTION_EXISTS`, `JOURNAL_ENTRY_EXISTS_FOR_TRANSACTION`, `UPSTREAM_JE_ERROR`, `UPSTREAM_JE_NETWORK_ERROR`.

## Future Enhancements

-   Integration with real bank APIs (e.g., Plaid, Finicity).
-   Automated transaction categorization using ML.
-   Improved reconciliation features.
-   Advanced fraud detection rules and ML model integration.
