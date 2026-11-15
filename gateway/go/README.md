# Vimbai API Gateway

This is the Vimbai API Gateway, built with Go and the Echo web framework. It serves as a single entry point for all client applications, routing requests to the appropriate backend microservices and centralizing authentication and authorization.

## Features

-   **Unified API Endpoint:** Clients interact with a single endpoint (`http://localhost:8081` in development).
-   **Dynamic Request Routing:** Routes incoming requests to the booking and calculation services by URL path prefix based on URL path prefixes.
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

To run this service along with Neo4j and other Vimbai services, you need Docker and Docker Compose installed.

### 1. Prerequisites

-   [Docker](https://docs.docker.com/get-docker/)
-   [Docker Compose](https://docs.docker.com/compose/install/)
-   Go (version 1.20+) is needed if developing/running outside Docker.

### 2. Environment Variables

The API Gateway requires the following environment variables. In the compose estate they are set on the `gateway` service in `infrastructure/compose/compose.yaml`.

-   `PORT`: The port the gateway will listen on (default `8081`).
-   `JWT_SECRET`: The secret key used to sign and verify JWT tokens. **CRITICAL: This MUST match the `JWT_SECRET` the calculation API signs and verifies tokens with, or the gateway and the engine will disagree about who the caller is.**
-   `IDENTITY_SERVICE_URL`: Internal URL of the engine that answers `/identity` (e.g. `http://calculation-api:8099`). The prefix is not stripped, because the engine answers `/identity/<path>` itself.
-   `RATE_LIMIT_ENABLED`, `RATE_LIMIT_RPS`, `RATE_LIMIT_BURST`: the gateway's own request limit.

There are no per-service URL variables. Every route except the `/identity` door is read from `config/services.json`, the same
manifest the engine, the on-device runtime and the address audit share. This file used to list six, one per retired service, and
the routes they fed forwarded to hosts that no longer resolve: a gateway that published `/accounts` and `/budgets` while the
catalogue published the accounting surface the engine actually answers. If a prefix needs routing, add it there, where every
component reads it.

### 3. Running the Services (with Docker Compose)

From the root of the repository:

```bash
docker compose -f infrastructure/compose/compose.yaml up --build
```

This command will:
1.  Build the engine, the gateway, the worker and the sync server, and pull the database image.
2.  Start them, with the gateway in front of the engine.

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

#### Example: list the chart of accounts (the gateway requires a token)

**Endpoint:** `GET http://localhost:8081/accounting/accounts` (the catalogue routes `/accounting` to the engine, unstripped)
**Headers:**
-   `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
-   `Content-Type: application/json`

The bare `/accounts`, `/journal-entries`, `/budgets` and `/customers` addresses this file used to document were the retired
services' shape. The engine answers the same surfaces under `/accounting/...`, which is what the catalogue publishes.

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
export IDENTITY_SERVICE_URL="http://localhost:8099" # The engine, answering /identity

# Run
go run main.go
```

## What it deliberately does not do

-   No financial logic: no calculation, no ledger posting, no settlement. It authenticates, rate-limits and forwards.
-   No routing table of its own. A route that is not in `config/services.json` (or the `/identity` door) does not exist, and a
    gate fails the build if a static route names an address the engine does not answer.
-   Centralized error handling and response normalization.
-   Service discovery integration.
