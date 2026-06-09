from neo4j import AsyncSession
from typing import Optional, List, Dict, Any
from reporting_service.models import (
    DashboardCreate, DashboardUpdate, DashboardInDB,
    ReportTemplateCreate, ReportTemplateUpdate, ReportTemplateInDB,
    ReportGenerationRequest, ReportResult,
    ScheduledReportCreate, ScheduledReportInDB
)
from datetime import datetime
import uuid
import json

async def init_db_schema():
    """Initialize report schema"""
    pass

# --- Dashboard CRUD ---
async def create_dashboard(session: AsyncSession, user_id: str, dashboard_data: DashboardCreate) -> DashboardInDB:
    dashboard_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    props = dashboard_data.model_dump()
    props["id"] = dashboard_id
    props["user_id"] = user_id
    props["widgets"] = json.dumps(props.get("widgets", []))
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()

    query = """
    MATCH (u:User {id: $user_id})
    CREATE (d:Dashboard $props)
    CREATE (u)-[:OWNS_DASHBOARD]->(d)
    RETURN d
    """
    result = await session.run(query, user_id=user_id, props=props)
    record = await result.single()
    return _dashboard_from_neo4j(record["d"])

async def get_dashboard(session: AsyncSession, dashboard_id: str) -> Optional[DashboardInDB]:
    query = "MATCH (d:Dashboard {id: $id}) RETURN d"
    result = await session.run(query, id=dashboard_id)
    record = await result.single()
    if record:
        return _dashboard_from_neo4j(record["d"])
    return None

async def get_user_dashboards(session: AsyncSession, user_id: str) -> List[DashboardInDB]:
    query = """
    MATCH (u:User {id: $user_id})-[:OWNS_DASHBOARD]->(d:Dashboard)
    RETURN d
    ORDER BY d.name
    """
    result = await session.run(query, user_id=user_id)
    dashboards = []
    async for record in result:
        dashboards.append(_dashboard_from_neo4j(record["d"]))
    return dashboards

async def update_dashboard(session: AsyncSession, dashboard_id: str, dashboard_data: DashboardUpdate) -> Optional[DashboardInDB]:
    update_fields = dashboard_data.model_dump(exclude_unset=True)
    if not update_fields:
        return await get_dashboard(session, dashboard_id)

    update_fields["updated_at"] = datetime.now(timezone.utc).isoformat()
    if "widgets" in update_fields:
        update_fields["widgets"] = json.dumps(update_fields["widgets"])

    set_clauses = [f"d.{k} = ${k}" for k in update_fields.keys()]
    set_query_part = ", ".join(set_clauses)

    query = f"""
    MATCH (d:Dashboard {{id: $id}})
    SET {set_query_part}
    RETURN d
    """
    params = {"id": dashboard_id, **update_fields}
    result = await session.run(query, params)
    record = await result.single()
    if record:
        return _dashboard_from_neo4j(record["d"])
    return None

async def delete_dashboard(session: AsyncSession, dashboard_id: str) -> bool:
    query = """
    MATCH (d:Dashboard {id: $id})
    DETACH DELETE d
    """
    result = await session.run(query, id=dashboard_id)
    return result.consume().counters.nodes_deleted > 0

# --- Report Template CRUD ---
async def create_report_template(session: AsyncSession, user_id: str, template_data: ReportTemplateCreate) -> ReportTemplateInDB:
    template_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)
    updated_at = datetime.now(timezone.utc)

    props = template_data.model_dump()
    props["id"] = template_id
    props["created_by"] = user_id
    props["created_at"] = created_at.isoformat()
    props["updated_at"] = updated_at.isoformat()

    query = """
    CREATE (rt:ReportTemplate $props)
    RETURN rt
    """
    result = await session.run(query, props=props)
    record = await result.single()
    return _template_from_neo4j(record["rt"])

async def get_all_report_templates(session: AsyncSession) -> List[ReportTemplateInDB]:
    query = """
    MATCH (rt:ReportTemplate)
    RETURN rt
    ORDER BY rt.name
    """
    result = await session.run(query)
    templates = []
    async for record in result:
        templates.append(_template_from_neo4j(record["rt"]))
    return templates

async def get_report_template(session: AsyncSession, template_id: str) -> Optional[ReportTemplateInDB]:
    query = "MATCH (rt:ReportTemplate {id: $id}) RETURN rt"
    result = await session.run(query, id=template_id)
    record = await result.single()
    if record:
        return _template_from_neo4j(record["rt"])
    return None

async def delete_report_template(session: AsyncSession, template_id: str) -> bool:
    query = "MATCH (rt:ReportTemplate {id: $id}) DETACH DELETE rt"
    result = await session.run(query, id=template_id)
    return result.consume().counters.nodes_deleted > 0

# --- Report Execution ---
async def execute_report_query(session: AsyncSession, query: str, parameters: Dict[str, Any]) -> List[Dict[str, Any]]:
    """Execute a Cypher query and return results"""
    result = await session.run(query, **parameters)
    records = []
    async for record in result:
        rec = {}
        for key, value in record.items():
            if hasattr(value, 'iso_format'):
                rec[key] = value.iso_format()
            elif isinstance(value, list):
                rec[key] = [
                    v.iso_format() if hasattr(v, 'iso_format') else v
                    for v in value
                ]
            else:
                rec[key] = value
        records.append(rec)
    return records

async def execute_report(session: AsyncSession, request: ReportGenerationRequest) -> ReportResult:
    report_id = str(uuid.uuid4())
    generated_at = datetime.now(timezone.utc)

    if request.template_id:
        template = await get_report_template(session, request.template_id)
        if not template:
            raise ValueError(f"Template {request.template_id} not found")
        query = template.query_definition
    elif request.custom_query:
        query = request.custom_query
    else:
        raise ValueError("Either template_id or custom_query must be provided")

    try:
        data = await execute_report_query(session, query, request.parameters)
        row_count = len(data)

        # Calculate summary statistics
        summary = {}
        if data and len(data) > 0:
            numeric_fields = [k for k, v in data[0].items() if isinstance(v, (int, float))]
            for field in numeric_fields:
                values = [r[field] for r in data if isinstance(r.get(field), (int, float))]
                if values:
                    summary[field] = {
                        "count": len(values),
                        "sum": sum(values),
                        "avg": sum(values) / len(values),
                        "min": min(values),
                        "max": max(values)
                    }

        return ReportResult(
            report_id=report_id,
            generated_at=generated_at,
            query_executed=query,
            row_count=row_count,
            data=data,
            summary=summary if summary else None
        )
    except Exception as e:
        raise ValueError(f"Query execution failed: {str(e)}")

# --- Scheduled Reports ---
async def create_scheduled_report(session: AsyncSession, user_id: str, report_data: ScheduledReportCreate) -> ScheduledReportInDB:
    scheduled_id = str(uuid.uuid4())
    created_at = datetime.now(timezone.utc)

    props = report_data.model_dump()
    props["id"] = scheduled_id
    props["created_by"] = user_id
    props["recipients"] = json.dumps(props.get("recipients", []))
    props["created_at"] = created_at.isoformat()

    query = """
    CREATE (sr:ScheduledReport $props)
    RETURN sr
    """
    result = await session.run(query, props=props)
    record = await result.single()
    return _scheduled_report_from_neo4j(record["sr"])

async def get_scheduled_reports(session: AsyncSession, user_id: str) -> List[ScheduledReportInDB]:
    query = "MATCH (sr:ScheduledReport {created_by: $user_id}) RETURN sr"
    result = await session.run(query, user_id=user_id)
    reports = []
    async for record in result:
        reports.append(_scheduled_report_from_neo4j(record["sr"]))
    return reports

# --- Helper Functions ---
def _dashboard_from_neo4j(node: Dict[str, Any]) -> DashboardInDB:
    return DashboardInDB(
        id=node["id"],
        name=node["name"],
        description=node.get("description"),
        widgets=json.loads(node.get("widgets", "[]")) if isinstance(node.get("widgets"), str) else node.get("widgets", []),
        is_default=node.get("is_default", False),
        user_id=node.get("user_id"),
        created_at=datetime.fromisoformat(node["created_at"].iso_format()) if hasattr(node["created_at"], 'iso_format') else datetime.fromisoformat(node["created_at"]),
        updated_at=datetime.fromisoformat(node["updated_at"].iso_format()) if hasattr(node["updated_at"], 'iso_format') else datetime.fromisoformat(node["updated_at"])
    )

def _template_from_neo4j(node: Dict[str, Any]) -> ReportTemplateInDB:
    return ReportTemplateInDB(
        id=node["id"],
        name=node["name"],
        description=node.get("description"),
        query_definition=node["query_definition"],
        parameters_schema=node.get("parameters_schema", {}),
        output_format=node.get("output_format", "json"),
        is_shared=node.get("is_shared", False),
        created_by=node["created_by"],
        created_at=datetime.fromisoformat(node["created_at"].iso_format()) if hasattr(node["created_at"], 'iso_format') else datetime.fromisoformat(node["created_at"]),
        updated_at=datetime.fromisoformat(node["updated_at"].iso_format()) if hasattr(node["updated_at"], 'iso_format') else datetime.fromisoformat(node["updated_at"])
    )

def _scheduled_report_from_neo4j(node: Dict[str, Any]) -> ScheduledReportInDB:
    return ScheduledReportInDB(
        id=node["id"],
        name=node["name"],
        template_id=node["template_id"],
        parameters=node.get("parameters", {}),
        schedule=node["schedule"],
        recipients=json.loads(node.get("recipients", "[]")) if isinstance(node.get("recipients"), str) else node.get("recipients", []),
        is_active=node.get("is_active", True),
        created_by=node.get("created_by"),
        last_run_at=datetime.fromisoformat(node["last_run_at"].iso_format()) if node.get("last_run_at") else None,
        next_run_at=datetime.fromisoformat(node["next_run_at"].iso_format()) if node.get("next_run_at") else None,
        created_at=datetime.fromisoformat(node["created_at"].iso_format()) if hasattr(node["created_at"], 'iso_format') else datetime.fromisoformat(node["created_at"])
    )
