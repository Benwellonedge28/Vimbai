# Vimbai Web Frontend

This is the web-based administrative dashboard and user-facing application for Vimbai, built using React, TypeScript, Vite, and Tailwind CSS. It interacts with the Vimbai microservices through the API Gateway.

## Features

-   User authentication (Login, Register) via the Identity Service.
-   Displays Chart of Accounts from the Accounting Service.
-   Modern, responsive UI with Tailwind CSS.

## Architecture

The web frontend is a single-page application (SPA) that communicates exclusively with the API Gateway. It uses React Router for client-side navigation and Axios for making HTTP requests. Authentication is handled using JWT tokens stored in local storage.

## Getting Started

To run this service along with other Vimbai services, you need Docker and Docker Compose installed.

### 1. Prerequisites

-   [Docker](https://docs.docker.com/get-docker/)
-   [Docker Compose](https://docs.docker.com/compose/install/)
-   Node.js (LTS version) and npm/yarn if developing locally outside Docker.

### 2. Environment Variables

The web frontend uses environment variables via Vite. Configure `VITE_API_BASE_URL` in a `.env` file in the `web-frontend` directory or directly in `docker-compose.yml`.

-   `VITE_API_BASE_URL`: The URL of the Vimbai API Gateway (e.g., `http://localhost:8081`).

### 3. Running the Services (with Docker Compose)

Navigate to the root directory of the Vimbai project (where `docker-compose.yml` is located) and run:

```bash
docker-compose up --build
```

This command will:
1.  Build the `web-frontend` Docker image and other Vimbai Docker images.
2.  Start Neo4j and all microservice containers.

The Web Frontend will be accessible at `http://localhost:3000` (or port 80 if served directly by nginx).

### 4. Development

To run the web frontend locally (outside Docker Compose):

```bash
# Navigate to web-frontend directory
cd web-frontend

# Install dependencies
npm install

# Set environment variable (in a .env file or directly in your shell)
# VITE_API_BASE_URL=http://localhost:8081

# Run in development mode
npm run dev
```

## Future Enhancements

-   Implement full CRUD operations for all Vimbai entities.
-   Rich interactive dashboards for financial reporting.
-   Advanced UI components and data visualizations.
-   Integration with other Vimbai microservices (Finance, Multimodal, Banking).
