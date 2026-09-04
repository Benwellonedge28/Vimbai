# Design Document: expense-tracking-service

## 1. Overview
The `expense-tracking-service` tracks and categorizes business expenses: submission, approval/rejection, per-category summaries. It is a member of the `ap-ar-expenses-bracket` (port 9006, gateway path `/expense-tracking`).

## 2. Architecture
- **Framework:** FastAPI (Python)
- **Storage:** Neo4j (expenses stored as `Expense` nodes linked to the submitting `User` via `[:SUBMITTED]`)
- **Deployment:** Bracket container (Docker), mounted at the original gateway path
- **Observability:** Structured logging via Structlog, optional OpenTelemetry tracing

## 3. Data Isolation (Book scoping)
Every request is scoped to the calling user (from the `X-User-Id` header injected by the API gateway) and, when present, to the Book from `X-Book-ID` (membership verified by the gateway):
- Creates stamp `book_id` on the `Expense` node (`None` = personal scope)
- Reads/writes filter `WHERE $book_id IS NULL OR e.book_id = $book_id`
- Cross-Book access is invisible (404), matching the platform-wide Book isolation rollout

## 4. Key Components
- `main.py` - routes, CORS, Book-context middleware; self-bootstraps the `expense_tracking_service` package so bare imports work in brackets/uvicorn
- `crud.py` - Cypher CRUD, every query bound to the request's Book context
- `models.py` - Pydantic models and category/status enums
- `dependencies.py` - `get_user_id`, `get_db_session`, `book_id_var`
- `database.py` - pooled Neo4j connector
- `fake_neo4j.py` - test-only fake driver shared by the service suite and the repo-root integration tests

## 5. Endpoints
- `GET /` - health
- `POST /expenses` - submit an expense (status starts `pending`)
- `GET /expenses/{company_id}` - list with optional `category` / `status_filter` / `limit`
- `PUT /expenses/{expense_id}/approve?approver=` - approve
- `PUT /expenses/{expense_id}/reject?reason=` - reject (reason stored)
- `GET /summary/{company_id}` - totals by category and status

## 6. Error Handling
Standard HTTP status codes; domain errors (`ExpenseError` subclasses) are mapped to JSON `{"detail", "code"}` responses. Cross-user or cross-Book lookups return `404`.
