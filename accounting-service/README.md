# ... (existing content) ...

### **Journal Entry Endpoints**

**(Requires `accounting.write.journal_entries` or `accounting.read.journal_entries` or `accounting.delete.journal_entries` permissions)**

#### Create a New Journal Entry

**Endpoint:** `POST http://localhost:8081/journal-entries/` (via API Gateway)
**Headers:** `Content-Type: application/json`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Body (JSON):**
```json
{
  "entry_date": "2026-05-20T10:00:00Z",
  "description": "Recorded sales for the day",
  "reference_number": "SALES-001",
  "source_module": "Sales",
  "status": "pending", // NEW: Can specify initial status
  "lines": [
    {
      "account_number": "1010",
      "debit": 1500.00,
      "credit": 0.00,
      "description": "Cash from sales"
    },
    {
      "account_number": "4000",
      "debit": 0.00,
      "credit": 1500.00,
      "description": "Sales revenue"
    }
  ]
}
```
*Note: The `status` field is new and defaults to `pending` if not provided.*

#### Update Journal Entry Status

**Endpoint:** `PUT http://localhost:8081/journal-entries/{entry_id}/status` (via API Gateway)
**Headers:** `Content-Type: application/json`, `Authorization: Bearer <YOUR_JWT_TOKEN_HERE>`
**Permissions:** `accounting.write.journal_entries_status`
**Body (JSON):**
```json
{
  "status": "posted"
}
```
*Allows updating only the status of an existing journal entry.*

# ... (existing content) ...

## Error Handling

The service now employs custom exceptions and global exception handlers to provide more structured and informative error responses. Errors are returned in a consistent JSON format:
```json
{
  "detail": "Descriptive error message",
  "code": "ERROR_CODE_ENUM",
  "status_code": 404
}
```
Common error codes include: `NOT_FOUND`, `CONFLICT_ERROR`, `VALIDATION_ERROR`, `UNAUTHORIZED`, `FORBIDDEN`, `ACCOUNT_EXISTS`, `ACCOUNT_NOT_FOUND_IN_JE`, `UNBALANCED_JOURNAL_ENTRY`, `INVALID_DATE_RANGE`, `PYDANTIC_VALIDATION_ERROR`.

# ... (existing content) ...
