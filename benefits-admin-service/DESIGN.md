# Design Document: benefits-admin-service

## 1. Overview
The `benefits-admin-service` manages employee benefits: benefit plans (pension, medical, dental, life insurance, leave), employee enrollments, and leave accruals with running balances. It is a member of the `ap-ar-expenses-bracket` (port 9006, gateway path `/benefits-admin`).

## 2. Architecture
- **Framework:** FastAPI (Python)
- **Storage:** Neo4j (`BenefitPlan`, `BenefitEnrollment`, `LeaveAccrual` nodes linked to the creating `User` via `[:CREATED]`)
- **Deployment:** Bracket container (Docker), mounted at the original gateway path
- **Observability:** Structured logging via Structlog

## 3. Data Isolation (Book scoping)
Every request is scoped to the calling user (`X-User-Id`, injected by the API gateway) and, when present, to the Book (`X-Book-ID`, membership verified by the gateway):
- Creates stamp `book_id` on the node (`None` = personal scope)
- Reads/writes filter `WHERE $book_id IS NULL OR x.book_id = $book_id`
- Cross-Book and cross-User lookups are invisible (404 / empty lists)

## 4. Key Components
- `main.py` - routes, CORS, Book-context middleware; self-bootstraps the `benefits_admin_service` package so bare imports work in brackets/uvicorn
- `crud.py` - Cypher CRUD, every query bound to the request's Book context; leave balances computed from the latest accrual
- `models.py` - Pydantic models and plan/leave type validation
- `dependencies.py` - `get_user_id`, `get_db_session`, `book_id_var`
- `database.py` - pooled Neo4j connector
- `fake_neo4j.py` - test-only fake driver shared by the service suite and the repo-root integration tests

## 5. Endpoints
- `GET /` and `GET /health` - health
- `POST /plans` - create a benefit plan (query params, contract preserved)
- `GET /plans?plan_type=` - list plans
- `POST /enroll` - enroll an employee (404 invisible plan, 409 duplicate active enrollment)
- `GET /employee/{employee_id}/enrollments` - active enrollments
- `POST /leave/accrue` - record accrual; balance carries forward per (employee, leave_type)
- `GET /employee/{employee_id}/leave?leave_type=` - accrual history

## 6. Error Handling
Domain errors (`BenefitsError` subclasses) map to JSON `{"detail", "code"}`: 400-level validation for invalid plan/leave types, 404 for invisible plans, 409 for duplicate enrollment.
