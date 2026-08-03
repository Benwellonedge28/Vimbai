# Vimbai Supply Chain Service

This service, formerly the Invoicing Service, has been expanded to manage critical aspects of the supply chain, including customer invoicing, supplier relationships, inventory tracking, and purchase order management. This expanded scope makes it a central component for achieving **Global Supply Chain Optimization & Risk Mitigation**.

## Features

-   **Customer Management:** CRUD operations for managing customer records.
-   **Sales Invoice Management:** CRUD operations for creating and tracking sales invoices.
-   **Supplier Management:** CRUD operations for defining and managing supplier relationships. (NEW)
-   **Inventory Item Management:** CRUD operations for tracking product inventory, including stock levels and reorder points. (NEW)
-   **Purchase Order Management:** CRUD operations for creating and managing purchase orders, linking to suppliers and inventory items. (NEW)
-   **JWT Authentication and Role-Based Access Control (RBAC)** for all API endpoints.
-   **Robust Error Handling:** Standardized error responses with custom exceptions.

## Architecture

The Supply Chain Service is a FastAPI application that uses Neo4j to store customer, sales invoice, supplier, inventory item, and purchase order data. It communicates with other Vimbai microservices via the API Gateway using `httpx`.

## Getting Started

To run this service along with Neo4j and other Vimbai services, you need Docker and Docker Compose installed.

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

Navigate to the root directory of the Vimbai project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `supply-chain-service` and other Vimbai Docker images.
2.  Start a Neo4j container and all other Vimbai microservices.

The Supply Chain Service API endpoints will be accessible via the API Gateway at `http://localhost:8081`.

### 4. Interacting with the API (Authenticated Endpoints)

All endpoints require a valid JWT in the `Authorization: Bearer <token>` header.

#### **Step 1: Get a JWT Token**

Refer to the `identity-service/README.md` for instructions on how to register a user and obtain a JWT token.
Ensure the user has relevant `supply_chain.*` permissions, or `SUPER_ADMIN` role.

#### **Step 2: Make Authenticated Requests via API Gateway**

Use the copied JWT token in the `Authorization` header for all requests.

**Header:** `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`

---

### **Customer Endpoints**

**(Permissions: `supply_chain.read.customers`, `supply_chain.write.customers`, `supply_chain.delete.customers`)**

#### Create a New Customer
**Endpoint:** `POST http://localhost:8081/customers/` (via API Gateway)

#### Get a Specific Customer by ID
**Endpoint:** `GET http://localhost:8081/customers/{customer_id}`

#### Get All Customers
**Endpoint:** `GET http://localhost:8081/customers/`

#### Update an Existing Customer
**Endpoint:** `PUT http://localhost:8081/customers/{customer_id}`

#### Delete an Existing Customer
**Endpoint:** `DELETE http://localhost:8081/customers/{customer_id}`

---

### **Sales Invoice Endpoints**

**(Permissions: `supply_chain.read.sales_invoices`, `supply_chain.write.sales_invoices`, `supply_chain.delete.sales_invoices`)**

#### Create a New Sales Invoice
**Endpoint:** `POST http://localhost:8081/sales-invoices/` (via API Gateway)

#### Get a Specific Sales Invoice by ID
**Endpoint:** `GET http://localhost:8081/sales-invoices/{invoice_id}`

#### Get All Sales Invoices
**Endpoint:** `GET http://localhost:8081/sales-invoices/`

#### Update an Existing Sales Invoice
**Endpoint:** `PUT http://localhost:8081/sales-invoices/{invoice_id}`

#### Delete an Existing Sales Invoice
**Endpoint:** `DELETE http://localhost:8081/sales-invoices/{invoice_id}`

---

### **Supplier Endpoints (NEW)**

**(Permissions: `supply_chain.read.suppliers`, `supply_chain.write.suppliers`, `supply_chain.delete.suppliers`)**

#### Create a New Supplier
**Endpoint:** `POST http://localhost:8081/suppliers/` (via API Gateway)
**Body (JSON):**
```json
{
  "name": "Global Components Inc.",
  "contact_person": "Jane Doe",
  "email": "jane.doe@globalcomponents.com",
  "phone": "+1-555-123-4567",
  "address": "123 Main St, Anytown, USA",
  "tax_id": "ABC-12345"
}
```

#### Get a Specific Supplier by ID
**Endpoint:** `GET http://localhost:8081/suppliers/{supplier_id}`

#### Get All Suppliers
**Endpoint:** `GET http://localhost:8081/suppliers/`

#### Update an Existing Supplier
**Endpoint:** `PUT http://localhost:8081/suppliers/{supplier_id}`

#### Delete an Existing Supplier
**Endpoint:** `DELETE http://localhost:8081/suppliers/{supplier_id}`

---

### **Inventory Item Endpoints (NEW)**

**(Permissions: `supply_chain.read.inventory_items`, `supply_chain.write.inventory_items`, `supply_chain.delete.inventory_items`)**

#### Create a New Inventory Item
**Endpoint:** `POST http://localhost:8081/inventory-items/` (via API Gateway)
**Body (JSON):**
```json
{
  "name": "Widget A",
  "sku": "WA-001",
  "description": "High-quality widget for general use.",
  "unit_cost": 10.50,
  "unit_of_measure": "pcs",
  "current_stock": 100,
  "reorder_point": 20,
  "preferred_supplier_id": "uuid-of-supplier"
}
```

#### Get a Specific Inventory Item by ID
**Endpoint:** `GET http://localhost:8081/inventory-items/{item_id}`

#### Get All Inventory Items
**Endpoint:** `GET http://localhost:8081/inventory-items/`

#### Update an Existing Inventory Item
**Endpoint:** `PUT http://localhost:8081/inventory-items/{item_id}`

#### Delete an Existing Inventory Item
**Endpoint:** `DELETE http://localhost:8081/inventory-items/{item_id}`

---

### **Purchase Order Endpoints (NEW)**

**(Permissions: `supply_chain.read.purchase_orders`, `supply_chain.write.purchase_orders`, `supply_chain.delete.purchase_orders`)**

#### Create a New Purchase Order
**Endpoint:** `POST http://localhost:8081/purchase-orders/` (via API Gateway)
**Body (JSON):**
```json
{
  "supplier_id": "uuid-of-supplier",
  "order_date": "2026-05-21T10:00:00Z",
  "expected_delivery_date": "2026-06-15T00:00:00Z",
  "total_amount": 1050.00,
  "currency": "USD",
  "status": "approved",
  "notes": "Urgent order for Q3 production.",
  "items": [
    {
      "inventory_item_id": "uuid-of-inventory-item-1",
      "quantity": 50,
      "unit_price": 10.50,
      "line_total": 525.00
    },
    {
      "inventory_item_id": "uuid-of-inventory-item-2",
      "quantity": 100,
      "unit_price": 5.25,
      "line_total": 525.00
    }
  ]
}
```

#### Get a Specific Purchase Order by ID
**Endpoint:** `GET http://localhost:8081/purchase-orders/{po_id}`

#### Get All Purchase Orders
**Endpoint:** `GET http://localhost:8081/purchase-orders/`

#### Update an Existing Purchase Order
**Endpoint:** `PUT http://localhost:8081/purchase-orders/{po_id}`

#### Delete an Existing Purchase Order
**Endpoint:** `DELETE http://localhost:8081/purchase-orders/{po_id}`

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
Common error codes include: `NOT_FOUND`, `CONFLICT_ERROR`, `VALIDATION_ERROR`, `UNAUTHORIZED`, `FORBIDDEN`, `CUSTOMER_EXISTS`, `SALES_INVOICE_NOT_FOUND`, `SUPPLIER_NOT_FOUND`, `INVENTORY_ITEM_EXISTS`, `INVENTORY_ITEM_NOT_FOUND`, `PURCHASE_ORDER_NOT_FOUND`.

## Future Enhancements

-   Automated inventory reconciliation.
-   Advanced supply chain risk assessment (e.g., geopolitical, natural disaster impact).
-   Demand forecasting integration.
-   Supplier performance analytics.
