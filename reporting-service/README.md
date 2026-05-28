# FinAcc Reporting Service

Advanced reporting and analytics service for FinAcc financial management system.

## Features

- **Custom Report Templates**: Create reusable report definitions with Cypher queries
- **Interactive Dashboards**: Configure widgets with charts, tables, and metrics
- **Real-time Analytics**: Execute queries on graph data for insights
- **Scheduled Reports**: Automate report generation and distribution
- **Export Capabilities**: Export reports to JSON, CSV, or PDF formats

## API Endpoints

### Dashboards
- `POST /dashboards/` - Create dashboard
- `GET /dashboards/` - List user dashboards
- `GET /dashboards/{id}` - Get dashboard
- `DELETE /dashboards/{id}` - Delete dashboard

### Report Templates
- `POST /templates/` - Create template
- `GET /templates/` - List templates
- `GET /templates/{id}` - Get template

### Reports
- `POST /reports/execute` - Execute report
- `POST /scheduled-reports/` - Create scheduled report

## Configuration

| Variable | Description | Default |
|----------|-------------|---------|
| `NEO4J_URI` | Neo4j connection URI | `bolt://neo4j:7687` |
| `NEO4J_PASSWORD` | Neo4j password | `neo4j` |
| `JWT_SECRET` | JWT signing secret | - |
| `PORT` | Service port | `8007` |

## Getting Started

```bash
uv pip install -r requirements.txt
uvicorn main:app --reload --port 8007
```
