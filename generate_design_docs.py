import os

def generate_design_doc(service_dir, service_name):
    doc_path = os.path.join(service_dir, "DESIGN.md")
    
    content = f"""# Design Document: {service_name}

## 1. Overview
The `{service_name}` is a microservice within the Vimbai Financial Accounting Platform. It is responsible for handling specific financial logic, data processing, or integration tasks related to its domain.

## 2. Architecture
- **Framework:** FastAPI (Python) or Gin/Standard Library (Go)
- **Communication:** RESTful JSON APIs over HTTP
- **Deployment:** Containerised via Docker, orchestrated via Kubernetes
- **Observability:** Structured logging via Structlog, metrics via Prometheus

## 3. Key Components
- **API Layer:** Handles incoming HTTP requests and input validation.
- **Service Layer:** Contains the core business logic and financial calculations.
- **Data Access Layer:** Manages interactions with databases or external systems (if applicable).

## 4. Endpoints
- `GET /health`: Health check endpoint for load balancers and Kubernetes probes.
- Other domain-specific endpoints are defined in the OpenAPI schema (`/docs`).

## 5. Error Handling
The service uses standard HTTP status codes:
- `200 OK` / `201 Created` for successful operations
- `400 Bad Request` for validation errors or invalid financial inputs
- `401 Unauthorized` / `403 Forbidden` for missing or insufficient credentials
- `404 Not Found` when requested resources do not exist
- `500 Internal Server Error` for unexpected failures

## 6. Security
- All endpoints (except `/health`) require a valid JWT token issued by the `identity-service`.
- Data is encrypted in transit via TLS.
"""
    with open(doc_path, "w", encoding="utf-8") as f:
        f.write(content)

if __name__ == "__main__":
    base_dir = "/home/ubuntu/Vimbai"
    
    # Python services
    with open("/tmp/python_services.txt", "r") as f:
        python_services = [line.strip() for line in f if line.strip()]
        
    for service in python_services:
        generate_design_doc(os.path.join(base_dir, service), service)
        
    # Go services
    generate_design_doc(os.path.join(base_dir, "api-gateway"), "api-gateway")
    generate_design_doc(os.path.join(base_dir, "identity-service"), "identity-service")
    
    print(f"Generated design documents for {len(python_services) + 2} services.")
