# Vimbai Fraud Detection Service

This service provides capabilities for real-time and batch analysis of financial transactions to detect potential fraud using machine learning models and rule-based systems. It integrates with other Vimbai microservices to receive transaction data and flags suspicious activities for review.

## Features

-   **Transaction Analysis:** Receives structured transaction data and applies a fraud detection model.
-   **Fraud Flagging:** Flags transactions as 'low_risk', 'suspicious', or 'high_risk' with a corresponding fraud score.
-   **Persistent Storage:** Stores flagged transactions in Neo4j for auditing and investigation.
-   **API Endpoints:** Provides endpoints to analyze transactions, retrieve fraud flags, and update their status.
-   **JWT Authentication and Role-Based Access Control (RBAC)** for all API endpoints.
-   **Robust Error Handling:** Standardized error responses with custom exceptions.

## Architecture

The Fraud Detection Service is a FastAPI application that uses Neo4j as its database to store fraud flags. It loads a (simulated) machine learning model to predict fraud. Communication with other services (e.g., Banking Integration, Multimodal Pipeline) is expected to happen via the API Gateway.

## Getting Started

To run this service along with Neo4j and other Vimbai services, you need Docker and Docker Compose installed.

### 1. Prerequisites

-   [Docker](https://docs.docker.com/get-docker/)
-   [Docker Compose](https://docs.docker.com/compose/install/)

### 2. Environment Variables

The service requires the following environment variables. It's recommended to set these in a `.env` file at the root of the project where `docker-compose.yml` is located.

-   `NEO4J_URI`: Connection URI for the Neo4j database (e.g., `bolt://neo4j:7687`).
-   `NEO4J_USER`: Username for Neo4j.
-   `NEO4J_PASSWORD`: Password for Neo4J.
-   `JWT_SECRET`: A strong, random secret key for signing JWT tokens. **CRITICAL: This MUST match the `JWT_SECRET` used by the `identity-service` for token validation to work.**

### 3. Running the Services (with Docker Compose)

Navigate to the root directory of the Vimbai project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `fraud-detection-service` and other Vimbai Docker images.
2.  Start a Neo4j container and all other Vimbai microservices.

The Fraud Detection Service API endpoints will be accessible via the API Gateway at `http://localhost:8081`.

### 4. Interacting with the API (Authenticated Endpoints)

All endpoints require a valid JWT in the `Authorization: Bearer <token>` header.

#### **Step 1: Get a JWT Token**

Refer to the `identity-service/README.md` for instructions on how to register a user and obtain a JWT token.
Ensure the user has relevant `fraud_detection.*` permissions, or `SUPER_ADMIN` role.

#### **Step 2: Make Authenticated Requests via API Gateway**

Use the copied JWT token in the `Authorization` header for all requests.

**Header:** `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`

---

### **Fraud Detection Endpoints**

#### Analyze a Transaction for Fraud

**Endpoint:** `POST http://localhost:8081/fraud-detection/analyze-transaction/` (via API Gateway)
**Permissions:** `fraud_detection.analyze_transaction`
**Body (JSON):**
```json
{
  "transaction_id": "TRANS-20260521-001",
  "amount": 1500.75,
  "currency": "USD",
  "sender_account_id": "ACC-XYZ-123",
  "recipient_account_id": "ACC-ABC-456",
  "transaction_type": "purchase",
  "timestamp": "2026-05-21T10:30:00Z",
  "location_data": {"city": "New York", "country": "USA"},
  "device_info": {"ip_address": "192.168.1.1", "browser": "Chrome"},
  "previous_transactions_count_24h": 5,
  "avg_daily_transaction_amount_7d": 120.50
}
```

#### Get All Fraud Flags

**Endpoint:** `GET http://localhost:8081/fraud-detection/fraud-flags/` (via API Gateway)
**Permissions:** `fraud_detection.read_flags`

#### Get a Specific Fraud Flag by ID

**Endpoint:** `GET http://localhost:8081/fraud-detection/fraud-flags/{flag_id}` (via API Gateway)
**Permissions:** `fraud_detection.read_flags`

#### Update Fraud Flag Status

**Endpoint:** `PUT http://localhost:8081/fraud-detection/fraud-flags/{flag_id}/status` (via API Gateway)
**Permissions:** `fraud_detection.manage_flags`
**Body (JSON):**
```json
"investigating"
```
(Accepted statuses: "open", "investigating", "false_positive", "confirmed_fraud")

---

### 5. Development

If developing locally (outside Docker Compose), ensure you have Python (3.10+) installed and your environment variables (`NEO4J_URI`, `NEO4J_USER`, `NEO4J_PASSWORD`, `JWT_SECRET`) are correctly set.

```bash
# Navigate to fraud-detection-service directory
cd fraud-detection-service

# Install dependencies
pip install -r requirements.txt

# Run the service locally
uvicorn main:app --host 0.0.0.0 --port 8005
```

## Error Handling

The service employs custom exceptions and global exception handlers to provide structured and informative error responses:
```json
{
  "detail": "Descriptive error message",
  "code": "ERROR_CODE_ENUM",
  "status_code": 404
}
```
Common error codes include: `NOT_FOUND`, `CONFLICT_ERROR`, `VALIDATION_ERROR`, `UNAUTHORIZED`, `FORBIDDEN`, `PYDANTIC_VALIDATION_ERROR`.
