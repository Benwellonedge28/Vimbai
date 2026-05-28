# FinAcc Workflow Service

A comprehensive workflow orchestration service for the FinAcc financial management system. This service manages workflow definitions, approval chains, and instance execution.

## Features

- **Workflow Definitions**: Create and manage configurable workflow templates
- **Approval Chains**: Multi-step approval processes with role-based assignment
- **Instance Management**: Track workflow instances through their lifecycle
- **Notifications**: Automated notifications for pending approvals
- **Audit Trail**: Complete history of all workflow actions

## Architecture

The Workflow Service manages multi-step approval processes for financial transactions:

- `WorkflowDefinition`: Template for workflow with ordered steps
- `WorkflowInstance`: Active instance of a workflow for a specific entity
- `ApprovalStep`: Individual step requiring approval
- `ApprovalAction`: Record of approval/rejection action taken

## API Endpoints

### Workflow Definitions
- `POST /workflow-definitions/` - Create workflow definition
- `GET /workflow-definitions/` - List all definitions
- `GET /workflow-definitions/{id}` - Get definition by ID
- `PUT /workflow-definitions/{id}` - Update definition
- `DELETE /workflow-definitions/{id}` - Delete definition

### Workflow Instances
- `POST /workflow-instances/` - Start workflow instance
- `GET /workflow-instances/` - List all instances
- `GET /workflow-instances/{id}` - Get instance details
- `PUT /workflow-instances/{id}` - Update instance
- `DELETE /workflow-instances/{id}` - Cancel instance
- `POST /workflow-instances/{id}/complete-task` - Complete a task

### Notifications
- `GET /notifications/` - List user notifications
- `PUT /notifications/{id}/read` - Mark as read

## Data Model

Graph relationships:
- `(WorkflowTemplate)-[:HAS_STEP {order: 1}]->(ApprovalStep)`
- `(WorkflowInstance)-[:BASED_ON]->(WorkflowTemplate)`
- `(ApprovalAction)-[:PERFORMED_BY]->(User)`

## Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `NEO4J_URI` | Neo4j connection URI | `bolt://neo4j:7687` |
| `NEO4J_PASSWORD` | Neo4j password | `neo4j` |
| `JWT_SECRET` | JWT signing secret | - |
| `PORT` | Service port | `8006` |

## Getting Started

```bash
pip install -r requirements.txt
uvicorn main:app --reload --port 8006
```
