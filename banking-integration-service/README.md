# FinAcc Banking Integration Service

This service is responsible for managing connections to external bank accounts and fetching transaction data. It acts as a bridge between FinAcc and financial institutions, automating the import of transaction data for reconciliation and accounting purposes.

## Features

-   **Bank Account Management (CRUD):** Allows users to link and manage their bank accounts within FinAcc.
-   **Transaction Fetching (Mocked):** Simulates fetching recent transactions from a connected bank account.
-   **Transaction Storage:** Stores fetched transactions in Neo4j, linking them to the respective bank accounts.
-   **JWT Authentication and Role-Based Access Control (RBAC)** for all API endpoints.

## Architecture

The service uses FastAPI (Python) and Neo4j for persistent storage of bank account and transaction data. It communicates with external banking APIs (mocked for this implementation) to retrieve transaction details. All external communication from clients should go through the API Gateway.

## Getting Started

To run this service along with Neo4j and other FinAcc services, you need Docker and Docker Compose installed.

### 1. Prerequisites

-   [Docker](https://docs.docker.com/get-docker/)
-   [Docker Compose](https://docs.docker.com/compose/install/)

### 2. Environment Variables

The service requires the following environment variables. It's recommended to set these in a `.env` file at the root of the project where `docker-compose.yml` is located.

-   `NEO4J_URI`: The connection URI for your Neo4j database (e.g., `bolt://neo4j:7687` when running with Docker Compose).
-   `NEO4J_USER`: The username for Neo4j (e.g., `neo4j`).
-   `NEO4J_PASSWORD`: The password for Neo4j (e.g., `neo4j` for default Docker setup, **CHANGE THIS IN PRODUCTION**).
-   `JWT_SECRET`: A strong, random secret key for signing JWT tokens. **CRITICAL: This MUST match the `JWT_SECRET` used by the `identity-service` and all other microservices for token validation to work.**
-   `API_GATEWAY_URL`: The internal URL of the API Gateway (e.g., `http://api-gateway:8081` when running with Docker Compose). This is used for generating correct `tokenUrl` for `OAuth2PasswordBearer`.

### 3. Running the Services (with Docker Compose)

Navigate to the root directory of the FinAcc project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `banking-integration-service` and other FinAcc Docker images.
2.  Start a Neo4j container and all other FinAcc microservices.

The Banking Integration Service will be accessible via the API Gateway at `http://localhost:8081` (prefix `/banking/accounts`, etc.).

### 4. Interacting with the API (Authenticated Endpoints)

All endpoints now require a valid JWT in the `Authorization: Bearer <token>` header.

#### **Step 1: Get a JWT Token**

Refer to the `identity-service/README.md` for instructions on how to register a user and obtain a JWT token.
Ensure the user has relevant `banking.*` permissions, or `SUPER_ADMIN` role. For example, a `BANKING_ADMIN` role could have `banking.*` permissions.

#### **Step 2: Make Authenticated Requests via API Gateway**

Use the copied JWT token in the `Authorization` header for all requests.

**Header:** `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`

---

### **Bank Account Endpoints**

**(Requires `banking.write.accounts` or `banking.read.accounts` or `banking.delete.accounts` permissions)**

#### Create a New Bank Account

**Endpoint:** `POST http://localhost:8081/banking/accounts/` (via API Gateway)
**Headers:** `Content-Type: application/json`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Body (JSON):**
```json
{
  "bank_name": "My Awesome Bank",
  "account_name": "Main Checking",
  "account_id": "MAA-123-XYZ",
  "account_type": "checking",
  "currency": "USD",
  "current_balance": 1500.75,
  "is_synced": false
}
```

#### Get All Bank Accounts for User

**Endpoint:** `GET http://localhost:8081/banking/accounts/` (via API Gateway)

#### Get Bank Account by ID

**Endpoint:** `GET http://localhost:8081/banking/accounts/{account_id}` (via API Gateway)
Example: `GET http://localhost:8081/banking/accounts/MAA-123-XYZ`

#### Update Bank Account

**Endpoint:** `PUT http://localhost:8081/banking/accounts/{account_id}` (via API Gateway)
Example: `PUT http://localhost:8081/banking/accounts/MAA-123-XYZ`
**Body (JSON - partial update):**
```json
{
  "account_name": "Primary Checking Account",
  "is_synced": true
}
```

#### Delete Bank Account

**Endpoint:** `DELETE http://localhost:8081/banking/accounts/{account_id}` (via API Gateway)
Example: `DELETE http://localhost:8081/banking/accounts/MAA-123-XYZ`

---

### **Bank Transaction Endpoints**

**(Requires `banking.read.transactions` or `banking.fetch.transactions` permissions)**

#### Fetch and Store Transactions (Mocked)

**Endpoint:** `POST http://localhost:8081/banking/accounts/{account_id}/fetch-transactions` (via API Gateway)
Example: `POST http://localhost:8081/banking/accounts/MAA-123-XYZ/fetch-transactions`
*This endpoint will simulate fetching and storing a few new transactions.*

#### Get Transactions for Bank Account

**Endpoint:** `GET http://localhost:8081/banking/accounts/{account_id}/transactions` (via API Gateway)
Example: `GET http://localhost:8081/banking/accounts/MAA-123-XYZ/transactions`

---

### 5. Development

If developing locally (outside Docker Compose), ensure you have Python (3.10+) installed and your environment variables (`NEO4J_URI`, `NEO4J_USER`, `NEO4J_PASSWORD`, `JWT_SECRET`, `API_GATEWAY_URL`) are correctly set.

```bash
# Navigate to banking-integration-service directory
cd banking-integration-service

# Install dependencies
pip install -r requirements.txt

# Run locally
uvicorn main:app --host 0.0.0.0 --port 8003
```

## Future Enhancements

-   Integration with real external banking APIs (e.g., Plaid, Finicity, Open Banking APIs).
-   Automated categorization of transactions.
-   Intelligent reconciliation suggestions with the Accounting Service.
-   Handling webhooks from banking APIs for real-time updates.
