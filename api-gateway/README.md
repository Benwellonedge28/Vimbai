# FinAcc API Gateway

This is the FinAcc API Gateway, built with Go and the Echo web framework. It serves as a single entry point for all client applications, routing requests to the appropriate backend microservices and centralizing authentication and authorization.

## Features

-   **Unified API Endpoint:** Clients interact with a single endpoint (`http://localhost:8081` in development).
-   **Dynamic Request Routing:** Routes incoming requests to `identity-service`, `accounting-service`, `finance-service`, or `multimodal-pipeline-service` based on URL path prefixes.
-   **Centralized JWT Authentication:** Validates JWT tokens for all authenticated routes, offloading this concern from individual microservices.
-   **User Context Propagation:** Extracts user claims (ID, username, role, permissions) from valid JWTs and forwards them to downstream microservices via custom HTTP headers (e.g., `X-User-ID`, `X-User-Role`, `X-User-Permissions`). This enables RBAC in individual services.
-   **CORS Handling:** Configured to allow cross-origin requests.

## Architecture

The API Gateway acts as a reverse proxy. When a request comes in:
1.  It checks the `Authorization` header for a JWT token.
2.  If required by the route, it validates the token using the configured `JWT_SECRET`.
3.  It extracts user claims from the token.
4.  It determines the target microservice based on the request path (e.g., `/accounts` goes to Accounting Service, `/budgets` to Finance Service).
5.  It forwards the request to the target microservice, adding user claim headers.
6.  The microservice then handles the request, potentially using the forwarded user claims for its own internal RBAC.

## Getting Started

To run this service along with Neo4j and other FinAcc services, you need Docker and Docker Compose installed.

### 1. Prerequisites

-   [Docker](https://docs.docker.com/get-docker/)
-   [Docker Compose](https://docs.docker.com/compose/install/)
-   Go (version 1.20+) is needed if developing/running outside Docker.

### 2. Environment Variables

The API Gateway requires the following environment variables. It's recommended to set these in a `.env` file at the root of the project where `docker-compose.yml` is located.

-   `PORT`: The port the gateway will listen on (default `8081`).
-   `JWT_SECRET`: The secret key used to sign and verify JWT tokens. **CRITICAL: This MUST match the `JWT_SECRET` used by the `identity-service` and all other microservices for token validation to work.**
-   `IDENTITY_SERVICE_URL`: Internal URL of the Identity Service (e.g., `http://identity-service:8080`).
-   `ACCOUNTING_SERVICE_URL`: Internal URL of the Accounting Service (e.g., `http://accounting-service:8000`).
-   `FINANCE_SERVICE_URL`: Internal URL of the Finance Service (e.g., `http://finance-service:8001`).
-   `MULTIMODAL_SERVICE_URL`: Internal URL of the Multimodal Pipeline Service (e.g., `http://multimodal-pipeline-service:8002`).

### 3. Running the Services (with Docker Compose)

Navigate to the root directory of the FinAcc project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build all FinAcc microservices, including the `api-gateway` Docker image.
2.  Start Neo4j and all microservice containers.

The API Gateway will be accessible at `http://localhost:8081`.

### 4. Interacting with the API Gateway

Clients (e.g., the mobile app, Postman, curl) should now send all requests to the API Gateway's URL (`http://localhost:8081`) with the appropriate path prefix.

#### Example: Login (No Authentication required by Gateway)

**Endpoint:** `POST http://localhost:8081/identity/login` (Routes to `IDENTITY_SERVICE_URL/login`)
**Headers:** `Content-Type: application/json`
**Body:**
```json
{
  "username": "testuser",
  "password": "password123"
}
```
This will return a JWT token from the Identity Service.

#### Example: Get Chart of Accounts (Authentication required by Gateway)

**Endpoint:** `GET http://localhost:8081/accounts/` (Routes to `ACCOUNTING_SERVICE_URL/accounts/`)
**Headers:**
-   `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
-   `Content-Type: application/json`

---

### 5. Development

To run the API Gateway locally (outside Docker Compose):

```bash
# Navigate to api-gateway directory
cd api-gateway

# Download dependencies
go mod tidy

# Set environment variables (replace with your actual service URLs and secret)
export PORT=8081
export JWT_SECRET="your_super_secret_jwt_key"
export IDENTITY_SERVICE_URL="http://localhost:8080" # If running identity service locally
export ACCOUNTING_SERVICE_URL="http://localhost:8000" # If running accounting service locally
export FINANCE_SERVICE_URL="http://localhost:8001" # If running finance service locally
export MULTIMODAL_SERVICE_URL="http://localhost:8002" # If running multimodal service locally

# Run
go run main.go
```

## Future Enhancements

-   Implement more advanced gateway features: rate limiting, caching, logging, circuit breakers.
-   Fine-grained routing rules.
-   Centralized error handling and response normalization.
-   Service discovery integration.
