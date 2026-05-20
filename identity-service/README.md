# FinAcc Identity Service

This is the Identity Service microservice for the FinAcc application, responsible for user authentication, authorization, and role management. It is built in Go and uses Neo4j as its persistent data store.

## Features

- User Registration
- User Login (generates JWT token)
- Role-Based Access Control (RBAC) definitions
- Integration with Neo4j for user and role persistence

## Getting Started

To run this service, you need Docker and Docker Compose installed.

### 1. Prerequisites

- [Docker](https://docs.docker.com/get-docker/)
- [Docker Compose](https://docs.docker.com/compose/install/)

### 2. Environment Variables

The service requires the following environment variables, typically set in a `.env` file at the root of the project or directly in `docker-compose.yml`:

- `NEO4J_URI`: The connection URI for your Neo4j database (e.g., `bolt://neo4j:7687` when running with Docker Compose).
- `NEO4J_USER`: The username for Neo4j (e.g., `neo4j`).
- `NEO4J_PASSWORD`: The password for Neo4j (e.g., `neo4j` for default Docker setup, **CHANGE THIS IN PRODUCTION**).
- `JWT_SECRET`: A strong, random secret key for signing JWT tokens. **CRITICAL: Change `your_super_secret_jwt_key` in `utils/jwt_utils.go` and set this via environment variable!**

### 3. Running the Service (with Docker Compose)

Navigate to the root directory of the FinAcc project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `identity-service` Docker image.
2.  Start a Neo4j container.
3.  Start the `identity-service` container, connecting it to Neo4j.
4.  Seed the Neo4j database with predefined roles (SUPER_ADMIN, ACCOUNTANT, etc.).

The service will be accessible at `http://localhost:8080`.

### 4. Interacting with the API

You can use tools like `curl`, Postman, or Insomnia to interact with the service.

#### Register a new user

**Endpoint:** `POST http://localhost:8080/register`

**Body (JSON):**
```json
{
  "username": "testuser",
  "email": "test@example.com",
  "password": "password123",
  "role_name": "ACCOUNTANT" // Optional. If not provided, defaults to ACCOUNTANT.
}
```

#### Login a user

**Endpoint:** `POST http://localhost:8080/login`

**Body (JSON):**
```json
{
  "username": "testuser",
  "password": "password123"
}
```

This will return a JWT token, which you can then use to authenticate requests to other FinAcc services (once they are implemented).

### 5. Neo4j Browser

Once Neo4j is running, you can access the Neo4j Browser at `http://localhost:7474` (or `http://localhost:7687` for HTTP port if 7474 is not working).
Use credentials `neo4j` for both username and password (or whatever you set in `NEO4J_PASSWORD`).
You can then inspect the created `User` and `Role` nodes and their `HAS_ROLE` relationships.

### 6. Development

If developing locally (outside Docker Compose), ensure you have Go (1.22+) installed and your environment variables (`NEO4J_URI`, `NEO4J_USER`, `NEO4J_PASSWORD`) are correctly set.

```bash
# Navigate to identity-service directory
cd identity-service

# Run locally
go run main.go
```

Remember to install dependencies: `go mod tidy`

## Database Model

Users and Roles are stored as nodes in Neo4j:

- `(:User)` nodes with properties: `id`, `username` (unique), `password_hash`, `email`.
- `(:Role)` nodes with properties: `id`, `name` (unique), `permissions` (array of strings).
- `(:User)-[:HAS_ROLE]->(:Role)` relationships to link users to their assigned roles.

Unique constraints are enforced on `User.username` and `Role.name`.

## Future Enhancements

- Full integration of permissions into JWT for robust authorization middleware.
- Refresh token implementation.
- Multi-Factor Authentication (MFA).
- User profile management (e.g., update email, password reset).
- Integration with API Gateway for token validation.
