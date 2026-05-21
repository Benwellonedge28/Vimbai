# FinAcc Multimodal Pipeline Service

This service processes various input types (image, audio, text) for financial data extraction and automation. It can perform OCR on documents, ASR on audio, and extract relevant financial entities. It then integrates with other FinAcc services, such as the Accounting Service for journal entry creation and now the Fraud Detection Service for transaction analysis.

## Features

-   **Asynchronous Processing with RabbitMQ:** Heavy multimodal processing tasks (OCR, ASR, Journal Entry creation) are offloaded to a message queue for background processing.
-   **Task Status Tracking:** Clients can query the status of asynchronous tasks using a unique `task_id`.
-   **Automated Journal Entry Creation:** Automatically converts extracted multimodal data into a proposed (or directly created) Journal Entry in the Accounting Service.
-   **Fraud Detection Integration:** Automatically sends extracted transaction data for fraud analysis upon processing. (NEW)
-   **JWT Authentication and Role-Based Access Control (RBAC)** for all API endpoints.
-   **Robust Error Handling:** Standardized error responses with custom exceptions.

## Architecture

The service leverages FastAPI for its API and `httpx` for internal, authenticated communication with other FinAcc microservices via the API Gateway. It integrates with **RabbitMQ** for asynchronous task processing, employing a producer/consumer model. The main FastAPI application acts as a producer, queuing tasks, while a background worker (or separate process) consumes and processes them.

## Getting Started

To run this service along with Neo4j, RabbitMQ, and other FinAcc services, you need Docker and Docker Compose installed.

### 1. Prerequisites

-   [Docker](https://docs.docker.com/get-docker/)
-   [Docker Compose](https://docs.docker.com/compose/install/)

### 2. Environment Variables

The service requires the following environment variables. It's recommended to set these in a `.env` file at the root of the project where `docker-compose.yml` is located.

-   `JWT_SECRET`: A strong, random secret key for signing JWT tokens. **CRITICAL: This MUST match the `JWT_SECRET` used by the `identity-service` for token validation to work.**
-   `API_GATEWAY_URL`: The internal URL of the API Gateway (e.g., `http://api-gateway:8081` when running with Docker Compose).
-   `RABBITMQ_HOST`: Hostname of the RabbitMQ server (e.g., `rabbitmq`).
-   `RABBITMQ_USER`: Username for RabbitMQ.
-   `RABBITMQ_PASS`: Password for RabbitMQ.

### 3. Running the Services (with Docker Compose)

Navigate to the root directory of the FinAcc project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `multimodal-pipeline-service` and other FinAcc Docker images.
2.  Start a Neo4j container, RabbitMQ, and all other FinAcc microservices.

The Multimodal Pipeline Service API endpoints will be accessible via the API Gateway at `http://localhost:8081`.

### 4. Interacting with the API (Authenticated Endpoints)

All endpoints now require a valid JWT in the `Authorization: Bearer <token>` header.

#### **Step 1: Get a JWT Token**

Refer to the `identity-service/README.md` for instructions on how to register a user and obtain a JWT token.
Ensure the user has relevant `multimodal.*` permissions, or `SUPER_ADMIN` role.

#### **Step 2: Make Authenticated Requests via API Gateway**

Use the copied JWT token in the `Authorization` header for all requests.

**Header:** `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`

---

### **Asynchronous Multimodal Processing Endpoints**

These endpoints now initiate background tasks and immediately return a `task_id` for status tracking.

#### Create Journal Entry from Multimodal Input

**Endpoint:** `POST http://localhost:8081/multimodal-to-journal-entry` (via API Gateway)
**Permissions:** `multimodal.create.journal_entry`
**Body (JSON):** (same as before, but now triggers async JE creation and fraud detection)
**Returns:** `TaskStatusResponse` with `task_id`.
*Note: This process now includes a call to the Fraud Detection Service, and the fraud analysis result will be included in the final `result` object of the task status.*

# ... (rest of the README.md content is unchanged) ...
