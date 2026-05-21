# ... (existing content) ...

### **Budget Endpoints**

**(Requires `finance.write.budgets` or `finance.read.budgets` or `finance.delete.budgets` permissions)**

#### Create a New Budget

**Endpoint:** `POST http://localhost:8081/budgets/` (via API Gateway)
**Permissions:** `finance.write.budgets`
**Headers:** `Content-Type: application/json`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Body (JSON):**
```json
{
  "name": "Q1 2026 Operational Budget",
  "start_date": "2026-01-01T00:00:00Z",
  "end_date": "2026-03-31T23:59:59Z",
  "currency": "USD",
  "description": "Quarterly budget for operational expenses and revenues."
}
```

#### Add a Budget Item to an Existing Budget

**Endpoint:** `POST http://localhost:8081/budgets/{budget_id}/items/` (via API Gateway)
**Permissions:** `finance.write.budget_items`
**Headers:** `Content-Type: application/json`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Body (JSON):**
```json
{
  "category": "Salaries",
  "account_number": "5000",
  "budgeted_amount": 150000.00,
  "budget_type": "expense"
}
```

#### Get a Specific Budget Item

**Endpoint:** `GET http://localhost:8081/budgets/{budget_id}/items/{item_id}` (via API Gateway)
**Permissions:** `finance.read.budget_items`
Example: `GET http://localhost:8081/budgets/a1b2c3d4-e5f6-7890-1234-567890abcdef/items/item-uuid`

#### Update a Specific Budget Item

**Endpoint:** `PUT http://localhost:8081/budgets/{budget_id}/items/{item_id}` (via API Gateway)
**Permissions:** `finance.write.budget_items`
**Body (JSON):**
```json
{
  "budgeted_amount": 160000.00
}
```

#### Delete a Specific Budget Item

**Endpoint:** `DELETE http://localhost:8081/budgets/{budget_id}/items/{item_id}` (via API Gateway)
**Permissions:** `finance.delete.budget_items`
Returns `204 No Content` on success.

# ... (rest of the Budget Endpoints and Financial Analysis Endpoints are unchanged) ...

## Error Handling

The service now employs custom exceptions and global exception handlers to provide more structured and informative error responses. Errors are returned in a consistent JSON format:
```json
{
  "detail": "Descriptive error message",
  "code": "ERROR_CODE_ENUM",
  "status_code": 404
}
```
Common error codes include: `NOT_FOUND`, `CONFLICT_ERROR`, `VALIDATION_ERROR`, `UNAUTHORIZED`, `FORBIDDEN`, `BUDGET_PERIOD_OVERLAP`, `BUDGET_ITEM_NOT_FOUND`, `UPSTREAM_IS_FETCH_FAILED`, `UPSTREAM_IS_NETWORK_ERROR`, `UPSTREAM_BS_FETCH_FAILED`, `UPSTREAM_BS_NETWORK_ERROR`.

# ... (rest of the README.md content is unchanged) ...
