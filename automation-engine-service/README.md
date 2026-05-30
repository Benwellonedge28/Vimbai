# FinAcc Automation Engine Service

The Automation Engine Service is responsible for managing and orchestrating automated tasks across the FinAcc microservices ecosystem. It provides a centralized system for scheduling, executing, and monitoring automated workflows.

## Features

- **Task Definition Management**: Create, update, delete, and manage automation task definitions
- **Scheduled Execution**: Support for cron-based and interval-based scheduling
- **Manual Triggering**: Manually trigger automation tasks via API
- **Task Instance Tracking**: Track execution status and results of individual task runs
- **Comprehensive Logging**: Detailed logging for debugging and audit purposes
- **Multi-Service Integration**: Can trigger actions across all FinAcc microservices

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Automation Engine Service                    │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │ Task        │  │ Scheduler    │  │ Executor            │  │
│  │ Definitions │  │ (APScheduler)│  │ (HTTP Client)       │  │
│  └──────┬──────┘  └──────┬───────┘  └──────────┬───────────┘  │
│         │               │                     │              │
│         └───────────────┴─────────────────────┘              │
│                         │                                    │
│                    ┌────┴────┐                               │
│                    │  Neo4j │                               │
│                    │   DB   │                               │
│                    └────────┘                               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    FinAcc Microservices                         │
├─────────────┬─────────────┬─────────────┬─────────────────────┤
│ Accounting  │  Finance    │  Banking    │  Other Services      │
│ Service     │  Service    │  Service    │                     │
└─────────────┴─────────────┴─────────────┴─────────────────────┘
```

## API Endpoints

### Task Definitions

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/task-definitions/` | Create a new task definition |
| GET | `/task-definitions/` | List all task definitions |
| GET | `/task-definitions/{id}` | Get a specific task definition |
| PUT | `/task-definitions/{id}` | Update a task definition |
| DELETE | `/task-definitions/{id}` | Delete a task definition |
| POST | `/task-definitions/{id}/trigger` | Manually trigger a task |

### Task Instances

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/task-instances/` | Create a new task instance |
| GET | `/task-instances/{id}` | Get a specific task instance |
| PUT | `/task-instances/{id}` | Update a task instance |

### Logs

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/task-instances/{id}/logs/` | Create a log entry |
| GET | `/task-instances/{id}/logs/` | Get logs for an instance |

## Task Definition Schema

```json
{
  "name": "Daily Bank Reconciliation",
  "description": "Automatically reconcile bank accounts daily at midnight",
  "service_target": "banking",
  "endpoint_path": "/banking/reconciliation/auto",
  "http_method": "POST",
  "payload_template": {},
  "schedule_type": "cron",
  "cron_schedule": "0 0 * * *",
  "is_active": true
}
```

## Schedule Types

- **manual**: Task can only be triggered manually
- **cron**: Task runs based on a cron expression (e.g., `0 0 * * *` for daily at midnight)
- **interval**: Task runs at regular intervals (e.g., every 3600 seconds)

## Example Usage

### Create a Task Definition

```bash
curl -X POST http://localhost:8006/task-definitions/ \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Weekly Report Generation",
    "description": "Generate financial reports every Monday",
    "service_target": "reporting",
    "endpoint_path": "/reports/generate",
    "http_method": "POST",
    "schedule_type": "cron",
    "cron_schedule": "0 8 * * 1"
  }'
```

### Manually Trigger a Task

```bash
curl -X POST http://localhost:8006/task-definitions/{id}/trigger \
  -H "Authorization: Bearer <token>"
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| NEO4J_URI | Neo4j database URI | bolt://localhost:7687 |
| NEO4J_USER | Neo4j username | neo4j |
| NEO4J_PASSWORD | Neo4j password | neo4j |
| JWT_SECRET | JWT signing secret | - |
| API_GATEWAY_URL | API Gateway URL | http://api-gateway:8081 |

## Docker Deployment

The service is containerized and can be deployed using Docker Compose:

```yaml
automation-engine-service:
  build: ./automation-engine-service
  environment:
    - NEO4J_URI=bolt://neo4j:7687
    - NEO4J_USER=neo4j
    - NEO4J_PASSWORD=${NEO4J_PASSWORD}
    - JWT_SECRET=${JWT_SECRET}
  depends_on:
    - neo4j
```

## Health Check

```bash
curl http://localhost:8006/
# Response: {"message": "FinAcc Automation Engine Service is running!"}
```

## License

MIT License - See FinAcc root README for details.