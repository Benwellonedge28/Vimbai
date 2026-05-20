# FinAcc Invoicing Service

This service manages customer information, creates and tracks invoices, and handles payment recording. It integrates with the Accounting Service to ensure all invoicing-related financial transactions are correctly recorded as Journal Entries.

## Features

-   **Customer Management (CRUD):** Create, Read, Update, Delete customer records.
-   **Invoice Management (CRUD):** Create, Read, Update, Delete invoices, including line items.
-   **Payment Recording:** Record payments against invoices, automatically generating Journal Entries in the Accounting Service.
-   **Integration with Accounting Service:** Seamlessly creates Journal Entries for sales and payments.
-   **JWT Authentication and Role-Based Access Control (RBAC)** for all API endpoints.

## Architecture

The service uses FastAPI (Python) and Neo4j for persistent storage of customer, invoice, and invoice item data. It communicates with the Accounting Service via the API Gateway to post financial transactions. All external communication from clients should go through the API Gateway.

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
-   `API_GATEWAY_URL`: The internal URL of the API Gateway (e.g., `http://api-gateway:8081` when running with Docker Compose). This is used for internal service-to-service communication.

### 3. Running the Services (with Docker Compose)

Navigate to the root directory of the FinAcc project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `invoicing-service` and other FinAcc Docker images.
2.  Start a Neo4j container and all other FinAcc microservices.

The Invoicing Service will be accessible via the API Gateway at `http://localhost:8081` (prefix `/invoicing/customers`, `/invoicing/invoices`, etc.).

### 4. Interacting with the API (Authenticated Endpoints)

All endpoints now require a valid JWT in the `Authorization: Bearer <token>` header.

#### **Step 1: Get a JWT Token**

Refer to the `identity-service/README.md` for instructions on how to register a user and obtain a JWT token.
Ensure the user has relevant `invoicing.*` permissions, or `SUPER_ADMIN` role. For example, a `SALES_MANAGER` role could have `invoicing.*` permissions.

#### **Step 2: Make Authenticated Requests via API Gateway**

Use the copied JWT token in the `Authorization` header for all requests.

**Header:** `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`

---

### **Customer Endpoints**

**(Requires `invoicing.write.customers` or `invoicing.read.customers` or `invoicing.delete.customers` permissions)**

#### Create a New Customer

**Endpoint:** `POST http://localhost:8081/invoicing/customers/` (via API Gateway)
**Headers:** `Content-Type: application/json`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Body (JSON):**
```json
{
  "name": "Global Tech Solutions",
  "email": "contact@globaltech.com",
  "phone": "+19876543210",
  "address": "456 Tech Park, Metropolis",
  "customer_id": "GT-001"
}
```

#### Get All Customers for User

**Endpoint:** `GET http://localhost:8081/invoicing/customers/` (via API Gateway)

#### Get Customer by ID

**Endpoint:** `GET http://localhost:8081/invoicing/customers/{customer_id}` (via API Gateway)
Example: `GET http://localhost:8081/invoicing/customers/GT-001`

#### Update Customer

**Endpoint:** `PUT http://localhost:8081/invoicing/customers/{customer_id}` (via API Gateway)
Example: `PUT http://localhost:8081/invoicing/customers/GT-001`
**Body (JSON - partial update):**
```json
{
  "email": "sales@globaltech.com"
}
```

#### Delete Customer

**Endpoint:** `DELETE http://localhost:8081/invoicing/customers/{customer_id}` (via API Gateway)
Example: `DELETE http://localhost:8081/invoicing/customers/GT-001`

---

### **Invoice Endpoints**

**(Requires `invoicing.write.invoices` or `invoicing.read.invoices` or `invoicing.delete.invoices` permissions)**

#### Create a New Invoice

**Endpoint:** `POST http://localhost:8081/invoicing/invoices/` (via API Gateway)
**Headers:** `Content-Type: application/json`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Body (JSON):**
```json
{
  "customer_id": "GT-001",
  "invoice_number": "INV-2026-0001",
  "invoice_date": "2026-05-20T10:00:00Z",
  "due_date": "2026-06-20T10:00:00Z",
  "total_amount": 1500.00,
  "status": "outstanding",
  "notes": "Project Alpha Phase 1",
  "items": [
    {
      "description": "Consulting Fee",
      "quantity": 1,
      "unit_price": 1500.00,
      "amount": 1500.00,
      "account_number": "4000"
    }
  ]
}
```

#### Get All Invoices for User

**Endpoint:** `GET http://localhost:8081/invoicing/invoices/` (via API Gateway)

#### Get Invoice by Number

**Endpoint:** `GET http://localhost:8081/invoicing/invoices/{invoice_number}` (via API Gateway)
Example: `GET http://localhost:8081/invoicing/invoices/INV-2026-0001`

#### Update Invoice

**Endpoint:** `PUT http://localhost:8081/invoicing/invoices/{invoice_number}` (via API Gateway)
Example: `PUT http://localhost:8081/invoicing/invoices/INV-2026-0001`
**Body (JSON - partial update):**
```json
{
  "status": "paid"
}
```

#### Delete Invoice

**Endpoint:** `DELETE http://localhost:8081/invoicing/invoices/{invoice_number}` (via API Gateway)
Example: `DELETE http://localhost:8081/invoicing/invoices/INV-2026-0001`

#### Record Payment for Invoice

**Endpoint:** `POST http://localhost:8081/invoicing/invoices/{invoice_number}/record-payment` (via API Gateway)
**Headers:** `Content-Type: application/json`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Body (JSON):**
```json
{
  "payment_amount": 1500.00,
  "payment_date": "2026-05-25T14:30:00Z"
}
```
*This will update the invoice status to "paid" and automatically create a Journal Entry in the Accounting Service.*

---

### 5. Development

If developing locally (outside Docker Compose), ensure you have Python (3.10+) installed and your environment variables (`NEO4J_URI`, `NEO4J_USER`, `NEO4J_PASSWORD`, `JWT_SECRET`, `API_GATEWAY_URL`) are correctly set.

```bash
# Navigate to invoicing-service directory
cd invoicing-service

# Install dependencies
pip install -r requirements.txt

# Run locally
uvicorn main:app --host 0.0.0.0 --port 8004
```

## Future Enhancements

-   Automated invoice generation from projects/tasks.
-   Recurring invoices.
-   Integration with payment gateways for actual payment processing.
-   Advanced reporting on accounts receivable aging.
-   Emailing invoices directly from the service.
