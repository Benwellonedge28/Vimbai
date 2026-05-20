# FinAcc Finance Service

This is the Finance Service microservice for the FinAcc application, built with FastAPI (Python) and utilizing Neo4j as its persistent data store. It is responsible for managing budgets, financial analysis, forecasting, and valuation.

## Features

- **Budget Management:**
    - Create, Read (single, all), Update, Delete budgets and their associated budget items.
    - Budgets can be categorized by fiscal year, period, and status.
- Neo4j integration for flexible data modeling of financial entities.
- **JWT Authentication and Role-Based Access Control (RBAC)** for all API endpoints.

## Getting Started

To run this service along with Neo4j, the Identity Service, and the Accounting Service, you need Docker and Docker Compose installed.

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

The Finance Service will be accessible at `http://localhost:8001`.

### 4. Interacting with the API (Authenticated Endpoints)

All endpoints now require a valid JWT in the `Authorization: Bearer <token>` header.

#### **Step 1: Get a JWT Token**

First, register and log in via the Identity Service:

1.  **Register a new user (if you haven't already):**
    **Endpoint:** `POST http://localhost:8080/register`
    **Body (JSON):**
    ```json
    {
      "username": "finance_user",
      "email": "finance@finacc.com",
      "password": "password123",
      "role_name": "FINANCE_LEAD"
    }
    ```
    (Or `SUPER_ADMIN` for full access)

2.  **Login to get a token:**
    **Endpoint:** `POST http://localhost:8080/login`
    **Body (JSON):**
    ```json
    {
      "username": "finance_user",
      "password": "password123"
    }
    ```
    This will return a `{"token": "eyJ..."}`. Copy this token.

#### **Step 2: Make Authenticated Requests to Finance Service**

Use the copied JWT token in the `Authorization` header for all requests to the Finance Service.

**Header:** `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`

**Permissions:**
- `FINANCE_LEAD` role has `finance.*` permissions (or more granular `finance.write.budgets`, `finance.read.budgets`, etc.), allowing operations on budgets.
- `SUPER_ADMIN` role has `*.*` permissions, allowing all operations.

---

### **Budget Endpoints**

**(Requires `finance.read.budgets` or `finance.write.budgets` or `finance.delete.budgets` permissions)**

#### Create a new Budget

**Endpoint:** `POST http://localhost:8001/budgets/`
**Body (JSON):**
```json
{
  "name": "Q3 2026 Marketing Budget",
  "fiscal_year": 2026,
  "period": "Q3",
  "description": "Budget for marketing activities in Q3 2026",
  "status": "Draft",
  "items": [
    {
      "category": "Advertising",
      "budgeted_amount": 5000.00,
      "actual_amount": 0.00,
      "description": "Online ad campaigns",
      "account_number": "5000",
      "period_start": "2026-07-01T00:00:00Z",
      "period_end": "2026-09-30T23:59:59Z"
    },
    {
      "category": "Events",
      "budgeted_amount": 2000.00,
      "actual_amount": 0.00,
      "description": "Sponsorship for industry events",
      "account_number": "5010",
      "period_start": "2026-07-01T00:00:00Z",
      "period_end": "2026-09-30T23:59:59Z"
    }
  ]
}
```
*Note: The `account_number` in budget items is a reference to accounts in the Accounting Service.*

#### Get Budget by ID

**Endpoint:** `GET http://localhost:8001/budgets/{budget_id}`
Example: `GET http://localhost:8001/budgets/a1b2c3d4-e5f6-7890-1234-567890abcdef`

#### Get all Budgets

**Endpoint:** `GET http://localhost:8001/budgets/`

#### Update a Budget

**Endpoint:** `PUT http://localhost:8001/budgets/{budget_id}`
Example: `PUT http://localhost:8001/budgets/a1b2c3d4-e5f6-7890-1234-567890abcdef`

**Body (JSON - partial update for the main budget details):**
```json
{
  "status": "Approved"
}
```
*Note: Updating budget items (adding/removing/modifying) would typically require separate endpoints for specific budget items or a more complex update logic.*

#### Delete a Budget

**Endpoint:** `DELETE http://localhost:8001/budgets/{budget_id}`
Example: `DELETE http://localhost:8001/budgets/a1b2c3d4-e5f6-7890-1234-567890abcdef`

### 5. Development

If developing locally (outside Docker Compose), ensure you have Python (3.10+) installed and your environment variables (`NEO4J_URI`, `NEO4J_USER`, `NEO4J_PASSWORD`, `JWT_SECRET`) are correctly set.

```bash
# Navigate to finance-service directory
cd finance-service

# Install dependencies
pip install -r requirements.txt

# Run locally
uvicorn main:app --host 0.0.0.0 --port 8001
```

## Database Model

Budgets are stored as `(:Budget)` nodes in Neo4j with properties `id`, `name` (unique), `fiscal_year`, `period`, `description`, `status`, `created_at`, `updated_at`.
Each budget has multiple `(:BudgetItem)` nodes linked via `(:Budget)-[:HAS_ITEM]->(:BudgetItem)`.
Each `(:BudgetItem)` node has properties `id` (unique), `category`, `budgeted_amount`, `actual_amount`, `description`, `account_number`, `period_start`, `period_end`, `created_at`, `updated_at`.

Unique constraints are enforced on `Budget.name` and `BudgetItem.id`.

## Future Enhancements

-   Financial Analysis (e.g., variance analysis against actuals from Accounting Service).
-   Forecasting and Scenario Modeling endpoints.
-   Capital Budgeting tools.
-   Integration with Accounting Service for pulling actuals.
